//! O **view-model do controle mensal** (spec 11, "Janela"): tudo o que a tela decide sem
//! desenhar, para testar sem egui. Navegação de período (ciclo 20–19 ou mês civil, limite de
//! 360 meses), agrupamento em 3 níveis na ordem de prioridade, subtotais que batem com as
//! linhas, cartões do resumo, rótulos de estado e a montagem das [`Alteracao`] dos diálogos.
//!
//! Regras de texto: nunca "sobra"; o livre é sempre uma **estimativa** quando o resumo diz;
//! a cor nunca é o único sinal (todo estado tem ícone e texto); "sem valor" nunca vira zero.

use sbf_planejamento::{
    Alteracao, Centavos, Ciclo, Competencia, Controle, Data, DiaDeCorte, EstadoDaOcorrencia,
    Excecao, HORIZONTE_MESES, IdOcorrencia, Natureza, Ocorrencia, Periodo, Proposta, Recorrente,
    Resumo, Saldo, Sentido,
};

use serde::{Deserialize, Serialize};

use crate::formato;

/// Quantos meses para trás a navegação deixa ir (conferir o que passou).
pub const MESES_PARA_TRAS: i32 = 24;
/// Quantos meses para a frente: o horizonte inteiro, contando o atual (`0..=359`).
pub const MESES_PARA_FRENTE: i32 = HORIZONTE_MESES as i32 - 1;

/// Quantas próximas ocorrências a aba mostra de início, e quantas "Mostrar mais" soma.
pub const PROXIMAS_DE_INICIO: u16 = 24;

const MESES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// `2026-11` → `11/2026`.
pub fn competencia_texto(c: Competencia) -> String {
    format!("{:02}/{:04}", c.mes, c.ano)
}

/// `2026-11-05` → `05/11/2026`.
pub fn data_texto(d: Data) -> String {
    format!("{:02}/{:02}/{:04}", d.dia, d.mes, d.ano)
}

/// `5/11` (sem o ano, para as notas curtas).
fn dia_mes(d: Data) -> String {
    format!("{:02}/{:02}", d.dia, d.mes)
}

/// `novembro de 2026`.
pub fn mes_por_extenso(c: Competencia) -> String {
    format!(
        "{} de {}",
        MESES[usize::from(c.mes.clamp(1, 12)) - 1],
        c.ano
    )
}

// ---------------------------------------------------------------------------------------
// Navegação de período
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoDoPeriodo {
    /// O ciclo 20–19 (a aba do mês na planilha). Padrão (D9).
    #[default]
    Ciclo,
    MesCivil,
}

impl ModoDoPeriodo {
    pub fn rotulo(self) -> &'static str {
        match self {
            ModoDoPeriodo::Ciclo => "Ciclo 20–19",
            ModoDoPeriodo::MesCivil => "Mês civil",
        }
    }
}

/// Onde o controle está no tempo. A âncora é a competência em que o período **começa**: o
/// ciclo `2026-11` vai de 20/11/2026 a 19/12/2026; o mês civil `2026-11` é o mês inteiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Navegador {
    pub modo: ModoDoPeriodo,
    ancora: Competencia,
    hoje: Data,
}

impl Navegador {
    pub fn novo(hoje: Data) -> Self {
        Self::no_modo(ModoDoPeriodo::Ciclo, hoje)
    }

    /// Para abrir a tela: o ciclo de hoje, ou o seguinte nos últimos 5 dias dele (o do
    /// próximo salário, decisão D9 da spec 11). "Hoje" continua levando ao ciclo de hoje.
    pub fn abertura(hoje: Data) -> Self {
        let mut n = Self::novo(hoje);
        if hoje.dias_ate(Ciclo::da_data(hoje, DiaDeCorte::PADRAO).fim) < 5 {
            n.ancora = n.ancora.seguinte();
        }
        n
    }

    fn no_modo(modo: ModoDoPeriodo, hoje: Data) -> Self {
        Navegador {
            modo,
            ancora: Self::base(modo, hoje),
            hoje,
        }
    }

    /// A competência do período que contém `hoje`.
    fn base(modo: ModoDoPeriodo, hoje: Data) -> Competencia {
        match modo {
            ModoDoPeriodo::Ciclo => {
                Competencia::de_data(Ciclo::da_data(hoje, DiaDeCorte::PADRAO).inicio)
            }
            ModoDoPeriodo::MesCivil => Competencia::de_data(hoje),
        }
    }

    pub fn hoje(&self) -> Data {
        self.hoje
    }

    /// Quantos meses a âncora está do período de hoje (negativo = passado).
    pub fn deslocamento(&self) -> i32 {
        Self::base(self.modo, self.hoje).meses_ate(self.ancora)
    }

    pub fn ancora(&self) -> Competencia {
        self.ancora
    }

    /// O primeiro e o último mês que a navegação alcança.
    pub fn limites(&self) -> (Competencia, Competencia) {
        let b = Self::base(self.modo, self.hoje);
        (
            b.somar_meses(-MESES_PARA_TRAS),
            b.somar_meses(MESES_PARA_FRENTE),
        )
    }

    pub fn periodo(&self) -> Periodo {
        match self.modo {
            ModoDoPeriodo::Ciclo => Periodo::Ciclo(Ciclo::do_mes(
                self.ancora.ano,
                self.ancora.mes,
                DiaDeCorte::PADRAO,
            )),
            ModoDoPeriodo::MesCivil => Periodo::MesCivil(self.ancora),
        }
    }

    pub fn pode_anterior(&self) -> bool {
        self.deslocamento() > -MESES_PARA_TRAS
    }

    pub fn pode_proximo(&self) -> bool {
        self.deslocamento() < MESES_PARA_FRENTE
    }

    /// Volta um período. Devolve `false` (e não muda) no limite.
    pub fn anterior(&mut self) -> bool {
        if !self.pode_anterior() {
            return false;
        }
        self.ancora = self.ancora.somar_meses(-1);
        true
    }

    /// Avança um período. Devolve `false` (e não muda) no limite dos 360 meses.
    pub fn proximo(&mut self) -> bool {
        if !self.pode_proximo() {
            return false;
        }
        self.ancora = self.ancora.somar_meses(1);
        true
    }

    /// "Hoje": o período que contém a data de hoje.
    pub fn ir_para_hoje(&mut self) {
        self.ancora = Self::base(self.modo, self.hoje);
    }

    /// "Ir para...": qualquer mês dos próximos 360 (e os 24 anteriores).
    pub fn ir_para(&mut self, competencia: Competencia) -> Result<(), String> {
        let (a, b) = self.limites();
        if competencia < a || competencia > b {
            return Err(format!(
                "Escolha um mês entre {} e {}.",
                competencia_texto(a),
                competencia_texto(b)
            ));
        }
        self.ancora = competencia;
        Ok(())
    }

    /// Troca ciclo × mês civil mantendo o período que cobre o meio do atual.
    pub fn trocar_modo(&mut self, modo: ModoDoPeriodo) {
        if modo == self.modo {
            return;
        }
        let p = self.periodo();
        let meio = p.inicio().somar_dias(p.inicio().dias_ate(p.fim()) / 2);
        self.modo = modo;
        let alvo = match modo {
            ModoDoPeriodo::Ciclo => {
                Competencia::de_data(Ciclo::da_data(meio, DiaDeCorte::PADRAO).inicio)
            }
            ModoDoPeriodo::MesCivil => Competencia::de_data(meio),
        };
        let (a, b) = self.limites();
        self.ancora = alvo.clamp(a, b);
    }

    /// A data de hoje mudou (virou o dia): o período escolhido fica onde está.
    pub fn atualizar_hoje(&mut self, hoje: Data) {
        self.hoje = hoje;
    }

    /// O texto do período: `Ciclo 20/11/2026 a 19/12/2026` ou `novembro de 2026`.
    pub fn rotulo(&self) -> String {
        match self.periodo() {
            Periodo::Ciclo(c) => format!("Ciclo {} a {}", data_texto(c.inicio), data_texto(c.fim)),
            Periodo::MesCivil(m) => mes_por_extenso(m),
        }
    }

    /// O período começa depois de hoje: o que se vê é projeção.
    pub fn e_projecao(&self) -> bool {
        self.periodo().inicio() > self.hoje
    }

    pub fn contem_hoje(&self) -> bool {
        self.periodo().contem(self.hoje)
    }
}

// ---------------------------------------------------------------------------------------
// Estados, origem, vencimento
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tom {
    Neutro,
    Ok,
    Aviso,
    Perigo,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstiloDoEstado {
    pub rotulo: &'static str,
    pub icone: &'static str,
    pub tom: Tom,
}

/// O estado como texto e ícone (a cor é só reforço).
pub fn estilo_do_estado(e: EstadoDaOcorrencia) -> EstiloDoEstado {
    use EstadoDaOcorrencia as E;
    let (rotulo, icone, tom) = match e {
        E::Prevista => ("Prevista", "schedule", Tom::Info),
        E::PagoInformado => ("Pago informado", "check_circle", Tom::Ok),
        E::Parcial => ("Parcial", "donut_small", Tom::Aviso),
        E::Conciliada => ("Conciliada", "verified", Tom::Ok),
        E::EmAtraso => ("Em atraso", "error", Tom::Perigo),
        E::Adiada => ("Adiada", "event_repeat", Tom::Aviso),
        E::Dispensada => ("Dispensada", "block", Tom::Neutro),
        E::SuprimidaPorPausa => ("Pausada", "pause_circle", Tom::Neutro),
        E::SuprimidaPorCancelamento => ("Cancelada", "cancel", Tom::Neutro),
    };
    EstiloDoEstado { rotulo, icone, tom }
}

pub fn suprimida(e: EstadoDaOcorrencia) -> bool {
    matches!(
        e,
        EstadoDaOcorrencia::SuprimidaPorPausa | EstadoDaOcorrencia::SuprimidaPorCancelamento
    )
}

/// A origem da linha: de onde veio o valor. No mês futuro, o que se vê é projeção da regra.
pub fn origem(o: &Ocorrencia, projecao: bool) -> (&'static str, &'static str) {
    use sbf_planejamento::Fonte;
    if projecao {
        return ("Projeção", "timeline");
    }
    match o.fonte {
        Fonte::Planilha => ("Planilha", "table_rows"),
        Fonte::Pluggy => ("Pluggy", "account_balance"),
        Fonte::Contrato => ("Contrato", "contract"),
        Fonte::Usuario => ("Usuário", "person"),
        Fonte::Ia => ("IA", "smart_toy"),
    }
}

/// A explicação da origem (tooltip).
pub fn origem_dica(o: &Ocorrencia, projecao: bool) -> String {
    use sbf_planejamento::{Certeza, Fonte};
    let fonte = match o.fonte {
        Fonte::Planilha => "a planilha",
        Fonte::Pluggy => "a média das transações da Pluggy",
        Fonte::Contrato => "o contrato",
        Fonte::Usuario => "você",
        Fonte::Ia => "uma sugestão da IA",
    };
    let certeza = match o.certeza {
        Certeza::Confirmado => "confirmado",
        Certeza::Estimado => "estimado",
    };
    if projecao {
        format!("Projeção da regra, não um fato. O valor vem d{fonte} ({certeza}).")
    } else {
        format!("O valor vem d{fonte} ({certeza}).")
    }
}

/// A célula de vencimento: o dia, e quando há duas datas, a segunda embaixo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VencimentoVm {
    pub principal: String,
    pub secundario: Option<String>,
    pub dica: Option<String>,
}

pub fn vencimento(o: &Ocorrencia) -> VencimentoVm {
    if o.vencimento == o.vencimento_original {
        return VencimentoVm {
            principal: data_texto(o.vencimento),
            secundario: None,
            dica: None,
        };
    }
    if matches!(o.natureza, Natureza::Entrada) {
        // D4: a data de disponibilidade pode vir antes da do crédito; as duas aparecem e
        // nada presume o crédito.
        let (disp, cred) = if o.vencimento <= o.vencimento_original {
            (o.vencimento, o.vencimento_original)
        } else {
            (o.vencimento_original, o.vencimento)
        };
        return VencimentoVm {
            principal: data_texto(disp),
            secundario: Some(format!("crédito {}", dia_mes(cred))),
            dica: Some(format!(
                "Disponível em {}; crédito em {}.",
                data_texto(disp),
                data_texto(cred)
            )),
        };
    }
    VencimentoVm {
        principal: data_texto(o.vencimento),
        secundario: Some(format!("era {}", dia_mes(o.vencimento_original))),
        dica: Some(format!("Vencia em {}.", data_texto(o.vencimento_original))),
    }
}

/// O que falta pagar: `None` quando não há como dizer (sem valor) ou não é devido.
pub fn falta(o: &Ocorrencia) -> Option<Centavos> {
    if suprimida(o.estado) || o.estado == EstadoDaOcorrencia::Dispensada {
        return None;
    }
    let previsto = o.previsto?;
    Some((previsto - o.conciliado.or(o.pago_informado).unwrap_or_default()).ou_zero())
}

/// O dinheiro, ou "sem valor" (nunca zero).
/// `R$ 1.234,56`.
pub fn reais(c: Centavos) -> String {
    formato::dinheiro(c.0)
}

pub fn dinheiro_ou_sem_valor(v: Option<Centavos>) -> String {
    v.map_or_else(|| "sem valor".to_owned(), reais)
}

// ---------------------------------------------------------------------------------------
// Grade em 3 níveis
// ---------------------------------------------------------------------------------------

/// Os números de um bloco da grade (subgrupo, grupo ou o período inteiro).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Subtotais {
    pub linhas: usize,
    pub entradas: Centavos,
    pub saidas: Centavos,
    pub descontos_em_folha: Centavos,
    /// Dinheiro de passagem (fora de receita e despesa).
    pub passagem: Centavos,
    /// Compras no cartão: já estão dentro das faturas, então ficam à parte das saídas.
    pub no_cartao: Centavos,
    pub pago_informado: Centavos,
    pub conciliado: Centavos,
    pub falta: Centavos,
    pub sem_valor: u32,
}

impl Subtotais {
    pub fn somar_linha(&mut self, o: &Ocorrencia) {
        self.linhas += 1;
        if suprimida(o.estado) || o.estado == EstadoDaOcorrencia::Dispensada {
            return;
        }
        match o.previsto {
            None => self.sem_valor += 1,
            Some(p) => match o.natureza {
                Natureza::Entrada => self.entradas += p,
                Natureza::Saida => self.saidas += p,
                Natureza::DescontoEmFolha => self.descontos_em_folha += p,
                Natureza::Passagem { .. } => self.passagem += p,
                Natureza::DentroDaFatura => self.no_cartao += p,
            },
        }
        self.pago_informado += o.pago_informado.unwrap_or_default();
        self.conciliado += o.conciliado.unwrap_or_default();
        self.falta += falta(o).unwrap_or_default();
    }

    pub fn somar(&mut self, outro: &Subtotais) {
        self.linhas += outro.linhas;
        self.entradas += outro.entradas;
        self.saidas += outro.saidas;
        self.descontos_em_folha += outro.descontos_em_folha;
        self.passagem += outro.passagem;
        self.no_cartao += outro.no_cartao;
        self.pago_informado += outro.pago_informado;
        self.conciliado += outro.conciliado;
        self.falta += outro.falta;
        self.sem_valor += outro.sem_valor;
    }

    /// As partes do previsto que existem, cada uma com o nome (a coluna é uma só).
    pub fn partes_do_previsto(&self) -> Vec<(&'static str, Centavos)> {
        let mut v = Vec::new();
        if !self.entradas.e_zero() {
            v.push(("Entradas", self.entradas));
        }
        if !self.saidas.e_zero() {
            v.push(("Saídas", self.saidas));
        }
        if !self.descontos_em_folha.e_zero() {
            v.push(("Em folha", self.descontos_em_folha));
        }
        if !self.passagem.e_zero() {
            v.push(("Passagem", self.passagem));
        }
        if !self.no_cartao.e_zero() {
            v.push(("No cartão", self.no_cartao));
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgrupoVm {
    /// `None`: as linhas do grupo que não têm subgrupo.
    pub nome: Option<String>,
    pub linhas: Vec<Ocorrencia>,
    pub total: Subtotais,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrupoVm {
    pub nome: String,
    pub posicao: i64,
    pub subgrupos: Vec<SubgrupoVm>,
    pub total: Subtotais,
}

impl GrupoVm {
    pub fn linhas(&self) -> impl Iterator<Item = &Ocorrencia> {
        self.subgrupos.iter().flat_map(|s| s.linhas.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grade {
    pub grupos: Vec<GrupoVm>,
    pub total: Subtotais,
}

impl Grade {
    pub fn linhas(&self) -> impl Iterator<Item = &Ocorrencia> {
        self.grupos.iter().flat_map(|g| g.linhas())
    }

    /// A linha anterior e a seguinte, dentro do mesmo grupo, na ordem de prioridade.
    pub fn vizinhas(&self, id: &IdOcorrencia) -> (Option<&Ocorrencia>, Option<&Ocorrencia>) {
        for g in &self.grupos {
            let v: Vec<&Ocorrencia> = g.linhas().collect();
            if let Some(i) = v.iter().position(|o| &o.id == id) {
                // Mesmo recorrente em duas ocorrências (adiamento): ignora as do próprio.
                let antes = v[..i]
                    .iter()
                    .rev()
                    .find(|o| o.id.recorrente != id.recorrente);
                let depois = v[i + 1..].iter().find(|o| o.id.recorrente != id.recorrente);
                return (antes.copied(), depois.copied());
            }
        }
        (None, None)
    }
}

/// Agrupa as linhas em grupo → subgrupo → linha, na ordem de prioridade (D12): o grupo pela
/// sua `posicao`, o subgrupo pela menor `posicao` de suas linhas, a linha por `posicao` e
/// vencimento. Grupo que não está na lista de grupos vai para o fim, sem perder linha.
pub fn montar_grade(c: &Controle) -> Grade {
    let posicao_do_grupo = |nome: &str| {
        c.grupos
            .iter()
            .find(|g| g.nome == nome)
            .map_or(i64::MAX, |g| g.posicao)
    };
    let mut grupos: Vec<GrupoVm> = Vec::new();
    for o in &c.linhas {
        let i = match grupos.iter().position(|g| g.nome == o.grupo) {
            Some(i) => i,
            None => {
                grupos.push(GrupoVm {
                    nome: o.grupo.clone(),
                    posicao: posicao_do_grupo(&o.grupo),
                    subgrupos: Vec::new(),
                    total: Subtotais::default(),
                });
                grupos.len() - 1
            }
        };
        let g = &mut grupos[i];
        let j = match g.subgrupos.iter().position(|s| s.nome == o.subgrupo) {
            Some(j) => j,
            None => {
                g.subgrupos.push(SubgrupoVm {
                    nome: o.subgrupo.clone(),
                    linhas: Vec::new(),
                    total: Subtotais::default(),
                });
                g.subgrupos.len() - 1
            }
        };
        g.subgrupos[j].linhas.push(o.clone());
    }
    let chave_da_linha = |o: &Ocorrencia| (o.posicao, o.vencimento, o.nome.clone());
    for g in &mut grupos {
        for s in &mut g.subgrupos {
            s.linhas.sort_by_key(chave_da_linha);
            for o in &s.linhas {
                s.total.somar_linha(o);
            }
        }
        // O subgrupo vale pela sua linha mais prioritária.
        g.subgrupos
            .sort_by_key(|s| s.linhas.first().map(|o| o.posicao));
        let mut t = Subtotais::default();
        for s in &g.subgrupos {
            t.somar(&s.total);
        }
        g.total = t;
    }
    grupos.sort_by(|a, b| a.posicao.cmp(&b.posicao).then(a.nome.cmp(&b.nome)));
    let mut total = Subtotais::default();
    for g in &grupos {
        total.somar(&g.total);
    }
    Grade { grupos, total }
}

// ---------------------------------------------------------------------------------------
// Cartões do resumo
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartao {
    pub id: &'static str,
    pub titulo: &'static str,
    pub valor: String,
    pub notas: Vec<String>,
    /// "estimativa", "desatualizado"...
    pub selo: Option<&'static str>,
    pub dica: Option<String>,
    pub tom: Tom,
    /// Um aviso com triângulo (⚠): as rubricas do salário não batem com o digitado.
    pub aviso: Option<String>,
}

/// O saldo é de ontem ou mais velho que isso? (D10: passou de 1 dia, está desatualizado.)
pub fn saldo_desatualizado(s: &Saldo, hoje: Data) -> bool {
    s.quando
        .get(..10)
        .and_then(Data::de_iso)
        .is_none_or(|coleta| coleta.somar_dias(1) < hoje)
}

/// As notas do cartão "Disponível agora": uma linha por conta com saldo (somando as contas
/// de mesmo nome), a data da coleta mais recente uma vez só e quantas contas estão zeradas.
fn notas_dos_saldos(saldos: &[sbf_planejamento::Saldo], hoje: Data) -> Vec<String> {
    let mut por_conta: Vec<(String, Centavos)> = Vec::new();
    let mut zeradas = 0usize;
    for s in saldos {
        if s.valor == Centavos(0) {
            zeradas += 1;
            continue;
        }
        match por_conta.iter_mut().find(|(c, _)| *c == s.conta) {
            Some((_, v)) => *v += s.valor,
            None => por_conta.push((s.conta.clone(), s.valor)),
        }
    }
    let mut notas: Vec<String> = por_conta
        .into_iter()
        .map(|(c, v)| format!("{c} {}", reais(v)))
        .collect();
    if let Some(recente) = saldos.iter().max_by(|a, b| a.quando.cmp(&b.quando)) {
        notas.push(format!(
            "coleta {}{}",
            formato::data(&recente.quando),
            if saldo_desatualizado(recente, hoje) {
                " · desatualizado"
            } else {
                ""
            }
        ));
    }
    if zeradas > 0 {
        notas.push(format!(
            "+{zeradas} {} com saldo zero",
            if zeradas == 1 { "conta" } else { "contas" }
        ));
    }
    notas
}

/// Põe o triângulo de aviso no cartão "Entradas previstas": uma linha por entrada cujas
/// rubricas não batem com o digitado.
pub fn avisar_nos_cartoes(cartoes: &mut [Cartao], avisos: &[(String, String)]) {
    if avisos.is_empty() {
        return;
    }
    if let Some(c) = cartoes.iter_mut().find(|c| c.id == "entradas") {
        c.aviso = Some(
            avisos
                .iter()
                .map(|(nome, texto)| format!("{nome}: {texto}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

/// Os sete cartões do topo, na ordem. Cada número vem do [`Resumo`], rastreável às linhas.
pub fn cartoes(c: &Controle, hoje: Data) -> Vec<Cartao> {
    let r: &Resumo = &c.resumo;
    let mut v = Vec::new();

    // Disponível agora: por conta, com a data da coleta.
    let soma: Centavos = r.disponivel_agora.iter().map(|s| s.valor).sum();
    let desatualizado = r
        .disponivel_agora
        .iter()
        .any(|s| saldo_desatualizado(s, hoje));
    v.push(Cartao {
        id: "disponivel",
        titulo: "Disponível agora",
        valor: if r.disponivel_agora.is_empty() {
            "sem saldo".into()
        } else {
            reais(soma)
        },
        notas: notas_dos_saldos(&r.disponivel_agora, hoje),
        selo: desatualizado.then_some("desatualizado"),
        dica: Some(
            "O último saldo coletado da Pluggy, por conta. Passou de 1 dia, aparece desatualizado."
                .into(),
        ),
        aviso: None,
        tom: if desatualizado {
            Tom::Aviso
        } else {
            Tom::Neutro
        },
    });

    // Entradas previstas: com as duas datas do salário quando são diferentes.
    let notas: Vec<String> = c
        .linhas
        .iter()
        .filter(|o| {
            matches!(o.natureza, Natureza::Entrada) && o.vencimento != o.vencimento_original
        })
        .map(|o| {
            let v = vencimento(o);
            format!(
                "{}: disponível {} · {}",
                o.nome,
                dia_mes(o.vencimento.min(o.vencimento_original)),
                v.secundario.unwrap_or_default()
            )
        })
        .collect();
    v.push(Cartao {
        id: "entradas",
        titulo: "Entradas previstas",
        valor: reais(r.entradas_previstas),
        notas,
        selo: None,
        dica: Some("O que entra no período. A data de disponibilidade pode vir antes da data do crédito; as duas aparecem.".into()),
        aviso: None,
        tom: Tom::Neutro,
    });

    v.push(Cartao {
        id: "saidas",
        titulo: "Saídas de caixa",
        valor: reais(r.saidas_de_caixa_previstas),
        notas: Vec::new(),
        selo: None,
        dica: Some("Sem desconto em folha e sem dinheiro de passagem.".into()),
        aviso: None,
        tom: Tom::Neutro,
    });

    v.push(Cartao {
        id: "cartao",
        titulo: "Compras no cartão",
        valor: reais(r.compras_no_cartao),
        notas: vec!["Já dentro das faturas.".into()],
        selo: None,
        dica: Some(
            "Compras no cartão de crédito. O caixa sai pela fatura, que já está nas saídas; por isso não somam de novo."
                .into(),
        ),
        aviso: None,
        tom: Tom::Neutro,
    });

    v.push(Cartao {
        id: "folha",
        titulo: "Desconto em folha",
        valor: reais(r.descontos_em_folha),
        notas: vec!["À parte: não sai do caixa de novo.".into()],
        selo: None,
        dica: Some("O salário entra pelo líquido. Estas consignações aparecem, mas não entram nas saídas de caixa.".into()),
        aviso: None,
        tom: Tom::Neutro,
    });

    let antes = c.atrasadas_de_antes.len();
    v.push(Cartao {
        id: "atraso",
        titulo: "Em atraso",
        valor: reais(r.em_atraso),
        notas: if antes > 0 {
            vec![format!("Inclui {antes} de períodos anteriores.")]
        } else {
            Vec::new()
        },
        selo: None,
        dica: None,
        aviso: None,
        tom: if r.em_atraso > Centavos::ZERO {
            Tom::Perigo
        } else {
            Tom::Neutro
        },
    });

    v.push(Cartao {
        id: "conferir",
        titulo: "A conferir",
        valor: formato::inteiro(i64::from(r.a_conferir)),
        notas: Vec::new(),
        selo: None,
        dica: Some("Ocorrências sem valor, ou só com pago informado e sem conciliação.".into()),
        aviso: None,
        tom: if r.a_conferir > 0 {
            Tom::Aviso
        } else {
            Tom::Neutro
        },
    });

    let mut notas = Vec::new();
    if r.passagem_pendente > Centavos::ZERO {
        notas.push(format!(
            "Já tirado: {} de dinheiro de passagem a repassar.",
            reais(r.passagem_pendente)
        ));
    }
    v.push(Cartao {
        id: "livre",
        titulo: "Livre após compromissos",
        valor: reais(r.livre_apos_compromissos),
        notas,
        selo: r.livre_e_estimativa.then_some("estimativa"),
        dica: Some(if r.lacunas.is_empty() {
            "Disponível + entradas − saídas ainda não pagas − em atraso.".to_owned()
        } else {
            format!(
                "Disponível + entradas − saídas ainda não pagas − em atraso.\n\nO que falta para este número ficar certo:\n• {}",
                r.lacunas.join("\n• ")
            )
        }),
        aviso: None,
        tom: if r.livre_e_estimativa { Tom::Aviso } else { Tom::Neutro },
    });
    v
}

// ---------------------------------------------------------------------------------------
// Ações
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcaoDoControle {
    AdicionarCompromisso,
    AnexarDocumento,
    Editar,
    Pausar,
    Retomar,
    Cancelar,
    Adiar,
    Dispensar,
    InformarPago,
    ConfirmarConciliacao,
    Subir,
    Descer,
    Mover,
    Arquivar,
    Excluir,
    Restaurar,
    Propostas,
    /// Procura no extrato as conciliações sugeridas do período aberto.
    BuscarNoExtrato,
}

#[derive(Debug, Clone, Copy)]
pub struct AtalhoDoControle {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub tecla: egui::Key,
}

#[derive(Debug, Clone, Copy)]
pub struct DefinicaoDoControle {
    pub id: AcaoDoControle,
    pub codigo: &'static str,
    pub rotulo: &'static str,
    pub icone: &'static str,
    pub dica: &'static str,
    pub grupo: &'static str,
    pub atalho: AtalhoDoControle,
    pub keytip: &'static str,
}

const fn at(ctrl: bool, shift: bool, alt: bool, tecla: egui::Key) -> AtalhoDoControle {
    AtalhoDoControle {
        ctrl,
        shift,
        alt,
        tecla,
    }
}

use egui::Key;

/// A lista das Ações do controle mensal (Ribbon, menu da linha e Paleta leem daqui).
pub const ACOES_DO_CONTROLE: [DefinicaoDoControle; 18] = [
    DefinicaoDoControle {
        id: AcaoDoControle::AdicionarCompromisso,
        codigo: "controle.adicionar",
        rotulo: "Adicionar compromisso",
        icone: "add_circle",
        dica: "Cria um compromisso novo: nome, valor, frequência e prazo, com prévia",
        grupo: "Novo",
        atalho: at(true, false, true, Key::N),
        keytip: "N",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::AnexarDocumento,
        codigo: "controle.anexar-documento",
        rotulo: "Anexar documento",
        icone: "attach_file",
        dica: "Liga um documento do inventário a esta ocorrência ou ao compromisso inteiro",
        grupo: "Documentos",
        atalho: at(true, false, true, Key::A),
        keytip: "V",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Editar,
        codigo: "controle.editar",
        rotulo: "Editar",
        icone: "edit",
        dica: "Muda o valor ou os dados: só este mês ou daqui em diante",
        grupo: "Linha",
        atalho: at(false, false, false, Key::F2),
        keytip: "E",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::InformarPago,
        codigo: "controle.informar-pago",
        rotulo: "Informar pago",
        icone: "payments",
        dica: "Registra o que você pagou; a conciliação com o banco vem à parte",
        grupo: "Linha",
        atalho: at(true, true, false, Key::G),
        keytip: "G",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::ConfirmarConciliacao,
        codigo: "controle.confirmar-conciliacao",
        rotulo: "Confirmar conciliação",
        icone: "task_alt",
        dica: "Confirma o casamento sugerido com a transação do banco",
        grupo: "Linha",
        atalho: at(true, true, false, Key::K),
        keytip: "K",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Adiar,
        codigo: "controle.adiar",
        rotulo: "Adiar",
        icone: "event_repeat",
        dica: "Muda a data só deste mês; continua devido",
        grupo: "Esta ocorrência",
        atalho: at(true, true, false, Key::J),
        keytip: "A",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Dispensar,
        codigo: "controle.dispensar",
        rotulo: "Dispensar",
        icone: "block",
        dica: "Tira este mês do que é devido, com motivo (não é pagar)",
        grupo: "Esta ocorrência",
        atalho: at(true, true, false, Key::I),
        keytip: "D",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Pausar,
        codigo: "controle.pausar",
        rotulo: "Pausar",
        icone: "pause_circle",
        dica: "Suspende o compromisso por um tempo; o histórico fica",
        grupo: "Compromisso",
        atalho: at(true, true, false, Key::P),
        keytip: "P",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Retomar,
        codigo: "controle.retomar",
        rotulo: "Retomar",
        icone: "replay",
        dica: "Encerra a pausa a partir do mês escolhido",
        grupo: "Compromisso",
        atalho: at(true, true, false, Key::M),
        keytip: "R",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Cancelar,
        codigo: "controle.cancelar",
        rotulo: "Cancelar",
        icone: "cancel",
        dica: "Encerra o compromisso daqui em diante; o já devido continua",
        grupo: "Compromisso",
        atalho: at(true, true, false, Key::X),
        keytip: "C",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Arquivar,
        codigo: "controle.arquivar",
        rotulo: "Arquivar",
        icone: "folder_special",
        dica: "Tira da grade sem apagar nada",
        grupo: "Compromisso",
        atalho: at(true, true, false, Key::Q),
        keytip: "Q",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Excluir,
        codigo: "controle.excluir",
        rotulo: "Excluir",
        icone: "delete",
        dica: "Exclusão lógica, com motivo; dá para restaurar",
        grupo: "Compromisso",
        atalho: at(false, false, false, Key::Delete),
        keytip: "X",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Restaurar,
        codigo: "controle.restaurar",
        rotulo: "Restaurar",
        icone: "undo",
        dica: "Traz de volta um compromisso arquivado ou excluído",
        grupo: "Compromisso",
        atalho: at(true, true, false, Key::E),
        keytip: "T",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Subir,
        codigo: "controle.subir",
        rotulo: "Subir",
        icone: "arrow_upward",
        dica: "Dá mais prioridade (troca com a linha de cima do grupo)",
        grupo: "Prioridade",
        atalho: at(false, false, true, Key::ArrowUp),
        keytip: "U",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Descer,
        codigo: "controle.descer",
        rotulo: "Descer",
        icone: "arrow_downward",
        dica: "Dá menos prioridade (troca com a linha de baixo do grupo)",
        grupo: "Prioridade",
        atalho: at(false, false, true, Key::ArrowDown),
        keytip: "B",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Mover,
        codigo: "controle.mover",
        rotulo: "Mover de grupo",
        icone: "swap_vert",
        dica: "Leva o compromisso para outro grupo",
        grupo: "Prioridade",
        atalho: at(true, true, false, Key::B),
        keytip: "M",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::Propostas,
        codigo: "controle.propostas",
        rotulo: "Propostas do Claude",
        icone: "smart_toy",
        dica: "O que o Claude propôs: confirmar ou recusar só aqui",
        grupo: "Propostas",
        atalho: at(true, true, false, Key::L),
        keytip: "O",
    },
    DefinicaoDoControle {
        id: AcaoDoControle::BuscarNoExtrato,
        codigo: "controle.buscar-no-extrato",
        rotulo: "Buscar no extrato",
        icone: "search",
        dica: "Procura no extrato do banco o que casa com as ocorrências do período e deixa como sugestão",
        grupo: "Extrato",
        atalho: at(true, true, false, Key::Y),
        keytip: "S",
    },
];

pub fn definicao(a: AcaoDoControle) -> &'static DefinicaoDoControle {
    ACOES_DO_CONTROLE
        .iter()
        .find(|d| d.id == a)
        .unwrap_or(&ACOES_DO_CONTROLE[0])
}

pub fn acao_do_codigo(codigo: &str) -> Option<AcaoDoControle> {
    ACOES_DO_CONTROLE
        .iter()
        .find(|d| d.codigo == codigo)
        .map(|d| d.id)
}

/// O que a disponibilidade de uma Ação precisa saber da seleção.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextoDaLinha<'a> {
    pub linha: Option<&'a Ocorrencia>,
    pub recorrente: Option<&'a Recorrente>,
    pub primeira_do_grupo: bool,
    pub ultima_do_grupo: bool,
    /// Há um arquivado ou excluído escolhido no painel de arquivados.
    pub restauravel: Option<&'a Recorrente>,
}

/// `Ok` se a Ação vale agora; senão o motivo, em português.
pub fn disponibilidade(a: AcaoDoControle, c: &ContextoDaLinha<'_>) -> Result<(), String> {
    use EstadoDaOcorrencia as E;
    if matches!(
        a,
        AcaoDoControle::Propostas
            | AcaoDoControle::AdicionarCompromisso
            | AcaoDoControle::BuscarNoExtrato
    ) {
        return Ok(());
    }
    if a == AcaoDoControle::Restaurar {
        return c
            .restauravel
            .map(|_| ())
            .ok_or_else(|| "Escolha um compromisso na lista de arquivados e excluídos.".into());
    }
    let Some(o) = c.linha else {
        return Err("Escolha uma linha do controle.".into());
    };
    let rec = c.recorrente;
    let fora_do_ar = rec.is_some_and(|r| r.excluido.is_some());
    if fora_do_ar {
        return Err("O compromisso está excluído; restaure primeiro.".into());
    }
    let paga = matches!(o.estado, E::Conciliada | E::PagoInformado);
    match a {
        AcaoDoControle::Editar
        | AcaoDoControle::Mover
        | AcaoDoControle::Excluir
        | AcaoDoControle::AnexarDocumento => Ok(()),
        AcaoDoControle::Pausar => {
            if suprimida(o.estado) {
                Err("Este mês já está pausado ou cancelado.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Retomar => {
            if o.estado == E::SuprimidaPorPausa {
                Ok(())
            } else {
                Err("Só dá para retomar uma linha pausada.".into())
            }
        }
        AcaoDoControle::Cancelar => {
            if rec.is_some_and(|r| r.cancelamento.is_some())
                || o.estado == E::SuprimidaPorCancelamento
            {
                Err("O compromisso já está cancelado.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Adiar => {
            if suprimida(o.estado) || o.estado == E::Dispensada || o.estado == E::Conciliada {
                Err("Esta ocorrência não está devida.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Dispensar => {
            if suprimida(o.estado) || o.estado == E::Dispensada || paga {
                Err("Esta ocorrência já está paga, dispensada ou suspensa.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::InformarPago => {
            if suprimida(o.estado) || o.estado == E::Dispensada {
                Err("Esta ocorrência não está devida.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::ConfirmarConciliacao => {
            if o.sugestao_de_conciliacao {
                Ok(())
            } else {
                Err("Não há sugestão de conciliação nesta linha.".into())
            }
        }
        AcaoDoControle::Subir => {
            if c.primeira_do_grupo {
                Err("Já é a primeira do grupo.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Descer => {
            if c.ultima_do_grupo {
                Err("Já é a última do grupo.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Arquivar => {
            if rec.is_some_and(|r| r.arquivado) {
                Err("O compromisso já está arquivado.".into())
            } else {
                Ok(())
            }
        }
        AcaoDoControle::Restaurar
        | AcaoDoControle::Propostas
        | AcaoDoControle::BuscarNoExtrato
        | AcaoDoControle::AdicionarCompromisso => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------
// Diálogos: a Alteração que cada um monta
// ---------------------------------------------------------------------------------------

/// Sobre o que o diálogo age.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlvoDoDialogo {
    pub recorrente: String,
    pub nome: String,
    pub versao_base: i64,
    /// A ocorrência que estava selecionada (vazio ao restaurar).
    pub ocorrencia: Option<IdOcorrencia>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscopoEscolhido {
    SoEsteMes,
    DaquiEmDiante,
}

/// Os campos de cada diálogo, como texto digitado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formulario {
    Editar {
        escopo: EscopoEscolhido,
        valor: String,
        conta: String,
        centro: String,
        estimado: bool,
    },
    Pausar {
        desde: String,
        ate: String,
        motivo: String,
    },
    Retomar {
        desde: String,
    },
    Cancelar {
        desde: String,
        motivo: String,
        comprovado: bool,
    },
    Adiar {
        nova_data: String,
    },
    Dispensar {
        motivo: String,
    },
    InformarPago {
        valor: String,
        data: String,
    },
    Excluir {
        motivo: String,
        confirmou: bool,
    },
    Arquivar,
    Restaurar {
        excluido: bool,
    },
    Mover {
        grupo: String,
    },
}

/// `2026-11` ou `11/2026` → competência.
pub fn competencia_de_texto(t: &str) -> Option<Competencia> {
    let t = t.trim();
    if let Some((m, a)) = t.split_once('/') {
        return Competencia::nova(a.trim().parse().ok()?, m.trim().parse().ok()?);
    }
    Competencia::de_texto(t)
}

fn exigir_motivo(motivo: &str) -> Result<String, String> {
    let m = motivo.trim();
    if m.is_empty() {
        Err("Diga o motivo.".into())
    } else {
        Ok(m.to_owned())
    }
}

fn opcional(t: &str) -> Option<String> {
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// Monta a [`Alteracao`] do diálogo, ou diz o que está errado. `competencia` é o mês da
/// ocorrência selecionada.
pub fn montar_alteracao(
    alvo: &AlvoDoDialogo,
    competencia: Competencia,
    f: &Formulario,
    atual: &AtualDaVersao,
) -> Result<Alteracao, String> {
    let rec = alvo.recorrente.clone();
    let base = alvo.versao_base;
    match f {
        Formulario::Editar {
            escopo,
            valor,
            conta,
            centro,
            estimado,
        } => {
            use sbf_planejamento::{Certeza, Escopo, MudancaDeVersao};
            let novo_valor = if valor.trim().is_empty() {
                None
            } else {
                Some(Centavos(
                    crate::janela::centavos_de(valor)
                        .filter(|v| *v >= 0)
                        .ok_or("Valor inválido (use 312,44).")?,
                ))
            };
            let mut m = MudancaDeVersao::default();
            if novo_valor != atual.valor {
                m.valor = Some(novo_valor);
            }
            let escopo = match escopo {
                EscopoEscolhido::SoEsteMes => {
                    // "Só este mês" muda só o valor (vira uma exceção).
                    if conta.trim() != atual.conta || centro.trim() != atual.centro {
                        return Err(
                            "Conta e centro só mudam \"daqui em diante\". Escolha esse escopo."
                                .into(),
                        );
                    }
                    if m.valor.is_none() {
                        return Err("Mude o valor para aplicar só neste mês.".into());
                    }
                    if novo_valor.is_none() {
                        return Err("Só este mês precisa de um valor.".into());
                    }
                    Escopo::SoEsteMes(competencia)
                }
                EscopoEscolhido::DaquiEmDiante => {
                    if conta.trim() != atual.conta {
                        m.conta = Some(opcional(conta));
                    }
                    if centro.trim() != atual.centro {
                        m.centro = Some(opcional(centro));
                    }
                    let certeza = if *estimado {
                        Certeza::Estimado
                    } else {
                        Certeza::Confirmado
                    };
                    if certeza != atual.certeza {
                        m.certeza = Some(certeza);
                    }
                    if m == MudancaDeVersao::default() {
                        return Err("Nada mudou.".into());
                    }
                    Escopo::DaquiEmDiante(competencia)
                }
            };
            Ok(Alteracao::Editar {
                recorrente: rec,
                escopo,
                mudanca: m,
                versao_base: base,
            })
        }
        Formulario::Pausar { desde, ate, motivo } => {
            let desde = competencia_de_texto(desde).ok_or("Mês inicial inválido (use 12/2026).")?;
            let ate = if ate.trim().is_empty() {
                None
            } else {
                let a = competencia_de_texto(ate).ok_or("Mês final inválido (use 03/2027).")?;
                if a <= desde {
                    return Err("O mês final vem depois do inicial (o final não entra).".into());
                }
                Some(a)
            };
            Ok(Alteracao::Pausar {
                recorrente: rec,
                desde,
                ate,
                motivo: exigir_motivo(motivo)?,
                versao_base: base,
            })
        }
        Formulario::Retomar { desde } => Ok(Alteracao::Retomar {
            recorrente: rec,
            desde: competencia_de_texto(desde).ok_or("Mês inválido (use 03/2027).")?,
            versao_base: base,
        }),
        Formulario::Cancelar {
            desde,
            motivo,
            comprovado,
        } => Ok(Alteracao::Cancelar {
            recorrente: rec,
            desde: competencia_de_texto(desde).ok_or("Mês inválido (use 02/2027).")?,
            motivo: exigir_motivo(motivo)?,
            comprovado: *comprovado,
            versao_base: base,
        }),
        Formulario::Adiar { nova_data } => {
            let ocorrencia = alvo.ocorrencia.clone().ok_or("Escolha uma linha.")?;
            let nova = if nova_data.trim().is_empty() {
                None
            } else {
                Some(
                    crate::janela::data_iso(nova_data.trim())
                        .and_then(|d| Data::de_iso(&d))
                        .ok_or("Data inválida (use dd/mm/aaaa).")?,
                )
            };
            Ok(Alteracao::Excecao {
                ocorrencia,
                excecao: Excecao::Adiar { nova_data: nova },
            })
        }
        Formulario::Dispensar { motivo } => Ok(Alteracao::Excecao {
            ocorrencia: alvo.ocorrencia.clone().ok_or("Escolha uma linha.")?,
            excecao: Excecao::Dispensar {
                motivo: exigir_motivo(motivo)?,
            },
        }),
        Formulario::InformarPago { valor, data } => {
            let v = Centavos(
                crate::janela::centavos_de(valor)
                    .filter(|v| *v >= 0)
                    .ok_or("Valor inválido (use 312,44).")?,
            );
            let d = if data.trim().is_empty() {
                None
            } else {
                Some(
                    crate::janela::data_iso(data.trim())
                        .and_then(|d| Data::de_iso(&d))
                        .ok_or("Data inválida (use dd/mm/aaaa).")?,
                )
            };
            Ok(Alteracao::Excecao {
                ocorrencia: alvo.ocorrencia.clone().ok_or("Escolha uma linha.")?,
                excecao: Excecao::PagoInformado { valor: v, data: d },
            })
        }
        Formulario::Excluir { motivo, confirmou } => {
            let motivo = exigir_motivo(motivo)?;
            if !confirmou {
                return Err("Marque a confirmação para excluir.".into());
            }
            Ok(Alteracao::Excluir {
                recorrente: rec,
                motivo,
                versao_base: base,
            })
        }
        Formulario::Arquivar => Ok(Alteracao::Arquivar {
            recorrente: rec,
            arquivado: true,
            versao_base: base,
        }),
        Formulario::Restaurar { excluido } => {
            if *excluido {
                Ok(Alteracao::Restaurar {
                    recorrente: rec,
                    versao_base: base,
                })
            } else {
                Ok(Alteracao::Arquivar {
                    recorrente: rec,
                    arquivado: false,
                    versao_base: base,
                })
            }
        }
        Formulario::Mover { grupo } => {
            let g = grupo.trim();
            if g.is_empty() {
                return Err("Escolha o grupo.".into());
            }
            Ok(Alteracao::Mover {
                recorrente: rec,
                grupo: g.to_owned(),
                // No fim do grupo de destino.
                posicao: i64::MAX / 2,
                versao_base: base,
            })
        }
    }
}

/// Os dados da versão vigente que o diálogo de editar compara para saber o que mudou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtualDaVersao {
    pub valor: Option<Centavos>,
    pub conta: String,
    pub centro: String,
    pub certeza: sbf_planejamento::Certeza,
}

impl AtualDaVersao {
    pub fn de(o: &Ocorrencia) -> Self {
        AtualDaVersao {
            valor: o.previsto,
            conta: o.conta.clone().unwrap_or_default(),
            centro: o.centro.clone().unwrap_or_default(),
            certeza: o.certeza,
        }
    }
}

/// A troca de duas prioridades (subir ou descer): as duas [`Alteracao::Mover`], na ordem.
pub fn trocar_prioridade(a: &Recorrente, b: &Recorrente) -> [Alteracao; 2] {
    // Se as posições empatam, separa-as: a linha que sobe fica uma posição antes da outra.
    let (pa, pb) = if a.posicao == b.posicao {
        (a.posicao - 1, b.posicao)
    } else {
        (b.posicao, a.posicao)
    };
    [
        Alteracao::Mover {
            recorrente: a.id.clone(),
            grupo: a.grupo.clone(),
            posicao: pa,
            versao_base: a.versao_do_registro,
        },
        Alteracao::Mover {
            recorrente: b.id.clone(),
            grupo: b.grupo.clone(),
            posicao: pb,
            versao_base: b.versao_do_registro,
        },
    ]
}

// ---------------------------------------------------------------------------------------
// Texto em português
// ---------------------------------------------------------------------------------------

/// A alteração em português, para a lista de propostas e o histórico. `nome` traduz o id do
/// recorrente.
pub fn descrever(alteracao: &Alteracao, nome: &dyn Fn(&str) -> String) -> String {
    use sbf_planejamento::{Escopo, Frequencia};
    let escopo = |e: &Escopo| match e {
        Escopo::SoEsteMes(c) => format!("só em {}", competencia_texto(*c)),
        Escopo::DaquiEmDiante(c) => format!("a partir de {}", competencia_texto(*c)),
    };
    match alteracao {
        Alteracao::CriarRecorrente { recorrente, .. } => {
            format!("Criar o compromisso \u{201c}{}\u{201d}.", recorrente.nome)
        }
        Alteracao::Editar {
            recorrente,
            escopo: e,
            mudanca,
            ..
        } => {
            let mut partes = Vec::new();
            if let Some(v) = &mudanca.valor {
                partes.push(match v {
                    Some(v) => format!("o valor para {}", reais(*v)),
                    None => "o valor para \u{201c}sem valor\u{201d}".into(),
                });
            }
            if let Some(c) = &mudanca.conta {
                partes.push(format!(
                    "a conta para {}",
                    c.as_deref().unwrap_or("nenhuma")
                ));
            }
            if let Some(c) = &mudanca.centro {
                partes.push(format!(
                    "o centro para {}",
                    c.as_deref().unwrap_or("nenhum")
                ));
            }
            if mudanca.certeza.is_some() {
                partes.push("a certeza do valor".into());
            }
            if let Some(f) = &mudanca.frequencia {
                partes.push(match f {
                    Frequencia::Mensal { dia } => format!("o vencimento para o dia {dia}"),
                    _ => "a frequência".into(),
                });
            }
            let o_que = if partes.is_empty() {
                "os dados".into()
            } else {
                partes.join(", ")
            };
            format!(
                "Mudar {o_que} de \u{201c}{}\u{201d}, {}.",
                nome(recorrente),
                escopo(e)
            )
        }
        Alteracao::Pausar {
            recorrente,
            desde,
            ate,
            motivo,
            ..
        } => format!(
            "Pausar \u{201c}{}\u{201d} de {} {} ({motivo}).",
            nome(recorrente),
            competencia_texto(*desde),
            ate.map_or("até retomar".to_owned(), |a| format!(
                "até {} (exclusivo)",
                competencia_texto(a)
            )),
        ),
        Alteracao::Retomar {
            recorrente, desde, ..
        } => {
            format!(
                "Retomar \u{201c}{}\u{201d} em {}.",
                nome(recorrente),
                competencia_texto(*desde)
            )
        }
        Alteracao::Cancelar {
            recorrente,
            desde,
            motivo,
            comprovado,
            ..
        } => format!(
            "Cancelar \u{201c}{}\u{201d} a partir de {} ({motivo}; {}).",
            nome(recorrente),
            competencia_texto(*desde),
            if *comprovado {
                "com comprovante"
            } else {
                "sem comprovante do fornecedor"
            }
        ),
        Alteracao::Excecao {
            ocorrencia,
            excecao,
        } => {
            let n = nome(&ocorrencia.recorrente);
            let mes = competencia_texto(ocorrencia.competencia);
            match excecao {
                Excecao::Adiar { nova_data } => match nova_data {
                    Some(d) => format!(
                        "Adiar \u{201c}{n}\u{201d} de {mes} para {}.",
                        data_texto(*d)
                    ),
                    None => {
                        format!("Adiar \u{201c}{n}\u{201d} de {mes} sem data (continua devido).")
                    }
                },
                Excecao::Dispensar { motivo } => {
                    format!("Dispensar \u{201c}{n}\u{201d} em {mes} ({motivo}).")
                }
                Excecao::ValorSoDesteMes { valor } => {
                    format!(
                        "Mudar o valor de \u{201c}{n}\u{201d} só em {mes} para {}.",
                        reais(*valor)
                    )
                }
                Excecao::PagoInformado { valor, .. } => {
                    format!(
                        "Informar {} pagos em \u{201c}{n}\u{201d} ({mes}).",
                        reais(*valor)
                    )
                }
            }
        }
        Alteracao::DesfazerExcecao { .. } => "Desfazer uma exceção.".into(),
        Alteracao::Conciliar { conciliacao } => format!(
            "{} a conciliação de \u{201c}{}\u{201d} ({}) com {}.",
            if conciliacao.confirmada {
                "Confirmar"
            } else {
                "Sugerir"
            },
            nome(&conciliacao.ocorrencia.recorrente),
            competencia_texto(conciliacao.ocorrencia.competencia),
            reais(conciliacao.valor),
        ),
        Alteracao::Excluir {
            recorrente, motivo, ..
        } => {
            format!(
                "Excluir \u{201c}{}\u{201d} ({motivo}). Dá para restaurar.",
                nome(recorrente)
            )
        }
        Alteracao::Restaurar { recorrente, .. } => {
            format!("Restaurar \u{201c}{}\u{201d}.", nome(recorrente))
        }
        Alteracao::Arquivar {
            recorrente,
            arquivado,
            ..
        } => format!(
            "{} \u{201c}{}\u{201d}.",
            if *arquivado {
                "Arquivar"
            } else {
                "Tirar do arquivo"
            },
            nome(recorrente)
        ),
        Alteracao::Mover {
            recorrente, grupo, ..
        } => format!(
            "Mover \u{201c}{}\u{201d} para o grupo {grupo}.",
            nome(recorrente)
        ),
        Alteracao::CriarGrupo { nome, .. } => format!("Criar o grupo {nome}."),
        Alteracao::DefinirComposicao { composicao, .. } => format!(
            "Definir {} rubricas de {} a partir de {}.",
            composicao.rubricas.len(),
            nome(&composicao.recorrente),
            competencia_texto(composicao.desde)
        ),
        Alteracao::VincularDocumento { nome: doc, .. } => format!("Vincular o documento {doc}."),
        Alteracao::DesvincularDocumento { .. } => "Desvincular um documento.".to_owned(),
        Alteracao::MapearConta {
            apelido,
            conta_do_extrato: Some(c),
        } => format!("Ligar a conta {apelido} a {c}."),
        Alteracao::MapearConta { apelido, .. } => format!("Desligar a conta {apelido}."),
        Alteracao::Desconciliar { ocorrencia, .. } => {
            format!("Desfazer a conciliação de {ocorrencia}.")
        }
        Alteracao::DefinirAssinatura {
            assinatura: Some(a),
            ..
        } => format!("Reconhecer no extrato por \"{}\".", a.texto),
        Alteracao::DefinirAssinatura { .. } => "Tirar a assinatura do extrato.".to_owned(),
    }
}

/// Uma proposta como uma linha: quem propôs e o quê.
pub fn origem_da_proposta(p: &Proposta) -> &'static str {
    match p.origem.as_str() {
        "mcp" => "Claude (MCP)",
        "ocultos" => "Itens ocultos da Pluggy",
        "conciliacao" => "Conciliação",
        _ => "Outra origem",
    }
}

/// O sentido do dinheiro de passagem em texto (detalhe).
pub fn natureza_texto(n: Natureza) -> &'static str {
    match n {
        Natureza::Entrada => "Entrada",
        Natureza::Saida => "Saída de caixa",
        Natureza::DescontoEmFolha => "Desconto em folha",
        Natureza::Passagem {
            sentido: Sentido::Entrada,
        } => "Passagem (entrada)",
        Natureza::Passagem {
            sentido: Sentido::Saida,
        } => "Passagem (saída)",
        Natureza::DentroDaFatura => "Compra no cartão (dentro da fatura)",
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
    use sbf_planejamento::{Filtro, Planejamento};

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn controle_do(nav: &Navegador) -> Controle {
        PlanejamentoFicticio::novo(hoje())
            .controle(&nav.periodo(), Filtro::Todos, None, hoje())
            .expect("controle")
    }

    #[test]
    fn abre_no_ciclo_que_contem_hoje() {
        let n = Navegador::novo(hoje());
        assert_eq!(n.modo, ModoDoPeriodo::Ciclo);
        assert!(n.contem_hoje());
        assert!(!n.e_projecao());
        assert_eq!(n.rotulo(), "Ciclo 20/10/2026 a 19/11/2026");
    }

    #[test]
    fn anterior_proximo_e_hoje() {
        let mut n = Navegador::novo(hoje());
        assert!(n.proximo());
        assert_eq!(n.rotulo(), "Ciclo 20/11/2026 a 19/12/2026");
        assert!(n.e_projecao());
        assert!(n.anterior() && n.anterior());
        assert_eq!(n.deslocamento(), -1);
        n.ir_para_hoje();
        assert_eq!(n.deslocamento(), 0);
    }

    #[test]
    fn limite_de_360_meses_para_a_frente_e_24_para_tras() {
        let mut n = Navegador::novo(hoje());
        for _ in 0..MESES_PARA_FRENTE {
            assert!(n.proximo());
        }
        assert_eq!(n.deslocamento(), 359);
        assert!(!n.pode_proximo());
        assert!(!n.proximo(), "o 361º mês não existe");
        assert_eq!(n.deslocamento(), 359);
        let mut n = Navegador::novo(hoje());
        for _ in 0..MESES_PARA_TRAS {
            assert!(n.anterior());
        }
        assert!(!n.anterior());
    }

    #[test]
    fn ir_para_aceita_o_horizonte_e_recusa_o_resto() {
        let mut n = Navegador::novo(hoje());
        n.trocar_modo(ModoDoPeriodo::MesCivil);
        let h = Competencia::de_data(hoje());
        assert!(n.ir_para(Competencia { ano: 2050, mes: 1 }).is_ok());
        assert_eq!(n.rotulo(), "janeiro de 2050");
        assert!(n.e_projecao());
        assert!(n.ir_para(h.somar_meses(359)).is_ok());
        let e = n.ir_para(h.somar_meses(360)).unwrap_err();
        assert!(e.contains("Escolha um mês entre"));
        assert_eq!(n.ancora(), h.somar_meses(359), "não mudou ao recusar");
        assert!(n.ir_para(h.somar_meses(-25)).is_err());
    }

    #[test]
    fn trocar_o_modo_mantem_o_periodo_que_cobre_o_meio() {
        let mut n = Navegador::novo(hoje());
        // Ciclo 20/10 a 19/11: o meio cai em outubro/novembro; o mês civil que contém hoje.
        n.trocar_modo(ModoDoPeriodo::MesCivil);
        assert!(n.contem_hoje());
        n.trocar_modo(ModoDoPeriodo::Ciclo);
        assert!(n.contem_hoje());
        assert_eq!(n.deslocamento(), 0);
    }

    #[test]
    fn estados_tem_rotulo_e_icone_proprios() {
        use EstadoDaOcorrencia as E;
        let todos = [
            E::Prevista,
            E::PagoInformado,
            E::Parcial,
            E::Conciliada,
            E::EmAtraso,
            E::Adiada,
            E::Dispensada,
            E::SuprimidaPorPausa,
            E::SuprimidaPorCancelamento,
        ];
        let mut rotulos: Vec<_> = todos.iter().map(|e| estilo_do_estado(*e).rotulo).collect();
        rotulos.sort_unstable();
        rotulos.dedup();
        assert_eq!(rotulos.len(), todos.len(), "rótulos distintos");
        for e in todos {
            let s = estilo_do_estado(e);
            assert!(
                crate::icones::caractere(s.icone).is_some(),
                "ícone {}",
                s.icone
            );
            assert!(!s.rotulo.is_empty());
        }
        assert_eq!(estilo_do_estado(E::EmAtraso).rotulo, "Em atraso");
    }

    #[test]
    fn a_grade_agrupa_em_tres_niveis_na_ordem_de_prioridade() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let g = montar_grade(&c);
        let nomes: Vec<_> = g.grupos.iter().map(|x| x.nome.as_str()).collect();
        // A ordem dos grupos é a da lista de grupos (D12), não a alfabética.
        let esperado: Vec<_> = c
            .grupos
            .iter()
            .map(|x| x.nome.as_str())
            .filter(|n| nomes.contains(n))
            .collect();
        assert_eq!(nomes, esperado);
        assert_eq!(nomes[0], "Entradas");
        // Nível 2: o grupo Lar tem os subgrupos da planilha.
        let lar = g.grupos.iter().find(|x| x.nome == "Lar").expect("Lar");
        let subs: Vec<_> = lar
            .subgrupos
            .iter()
            .filter_map(|s| s.nome.as_deref())
            .collect();
        assert!(subs.contains(&"Moradia") && subs.contains(&"Contas"));
        // Nível 3: dentro do subgrupo, por posição.
        for gr in &g.grupos {
            for s in &gr.subgrupos {
                let pos: Vec<_> = s.linhas.iter().map(|o| o.posicao).collect();
                let mut ord = pos.clone();
                ord.sort_unstable();
                assert_eq!(pos, ord);
            }
        }
        // Não perde nenhuma linha.
        assert_eq!(g.linhas().count(), c.linhas.len());
    }

    #[test]
    fn totais_por_grupo_batem_com_as_linhas_e_com_o_rodape() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let g = montar_grade(&c);
        for gr in &g.grupos {
            let mut esperado = Subtotais::default();
            for o in gr.linhas() {
                esperado.somar_linha(o);
            }
            assert_eq!(gr.total, esperado, "grupo {}", gr.nome);
            let soma_sub: Centavos = gr.subgrupos.iter().map(|s| s.total.saidas).sum();
            assert_eq!(gr.total.saidas, soma_sub);
        }
        let mut tudo = Subtotais::default();
        for o in &c.linhas {
            tudo.somar_linha(o);
        }
        assert_eq!(g.total, tudo);
        // O rodapé também bate com o resumo do serviço (AC-12).
        assert_eq!(g.total.saidas, c.resumo.saidas_de_caixa_previstas);
        assert_eq!(g.total.entradas, c.resumo.entradas_previstas);
        assert_eq!(g.total.descontos_em_folha, c.resumo.descontos_em_folha);
    }

    #[test]
    fn sem_valor_nao_vira_zero_nos_totais() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let g = montar_grade(&c);
        assert!(g.total.sem_valor >= 1, "a energia está sem valor");
        let energia = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_energia")
            .expect("energia");
        assert_eq!(dinheiro_ou_sem_valor(energia.previsto), "sem valor");
        assert_eq!(falta(energia), None);
    }

    #[test]
    fn vencimento_mostra_as_duas_datas_do_salario() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let sal = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_salario")
            .expect("salário");
        let v = vencimento(sal);
        assert!(
            v.secundario
                .as_deref()
                .is_some_and(|s| s.starts_with("crédito "))
        );
        assert!(
            v.dica
                .as_deref()
                .is_some_and(|s| s.contains("Disponível em"))
        );
        let aluguel = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_aluguel")
            .expect("aluguel");
        assert_eq!(vencimento(aluguel).secundario, None);
    }

    #[test]
    fn vencimento_de_adiada_mostra_o_original() {
        let mut s = PlanejamentoFicticio::novo(hoje());
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let o = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_agua")
            .expect("água")
            .clone();
        s.aplicar(
            Alteracao::Excecao {
                ocorrencia: o.id.clone(),
                excecao: Excecao::Adiar {
                    nova_data: Some(hoje().somar_dias(3)),
                },
            },
            sbf_planejamento::Autor::Usuario,
        )
        .expect("adia");
        let c = s
            .controle(&n.periodo(), Filtro::Todos, None, hoje())
            .unwrap();
        let o = c
            .linhas
            .iter()
            .find(|o| o.id.recorrente == "rec_agua")
            .unwrap();
        assert_eq!(o.estado, EstadoDaOcorrencia::Adiada);
        let v = vencimento(o);
        assert!(
            v.secundario
                .as_deref()
                .is_some_and(|s| s.starts_with("era "))
        );
    }

    #[test]
    fn cartoes_na_ordem_e_com_selos() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let v = cartoes(&c, hoje());
        let ids: Vec<_> = v.iter().map(|x| x.id).collect();
        assert_eq!(
            ids,
            [
                "disponivel",
                "entradas",
                "saidas",
                "cartao",
                "folha",
                "atraso",
                "conferir",
                "livre"
            ]
        );
        let livre = v.iter().find(|x| x.id == "livre").unwrap();
        assert_eq!(livre.selo, Some("estimativa"));
        assert!(livre.dica.as_deref().unwrap().contains("desatualizado"));
        let disp = v.iter().find(|x| x.id == "disponivel").unwrap();
        assert_eq!(disp.selo, Some("desatualizado"));
        assert_eq!(
            disp.notas.len(),
            3,
            "uma nota por conta com saldo e a da coleta"
        );
        assert!(disp.notas.iter().any(|n| n.contains("coleta 16/11/2026")));
        let ent = v.iter().find(|x| x.id == "entradas").unwrap();
        assert!(
            ent.notas
                .iter()
                .any(|n| n.contains("disponível") && n.contains("crédito"))
        );
        let folha = v.iter().find(|x| x.id == "folha").unwrap();
        assert!(folha.notas[0].contains("não sai do caixa de novo"));
    }

    #[test]
    fn nenhum_texto_diz_sobra() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        for k in cartoes(&c, hoje()) {
            let tudo = format!(
                "{} {} {} {}",
                k.titulo,
                k.valor,
                k.notas.join(" "),
                k.dica.unwrap_or_default()
            )
            .to_lowercase();
            assert!(!tudo.contains("sobra"), "{tudo}");
        }
    }

    #[test]
    fn livre_certo_nao_leva_selo() {
        let mut c = Controle {
            periodo: Navegador::novo(hoje()).periodo(),
            grupos: Vec::new(),
            linhas: Vec::new(),
            atrasadas_de_antes: Vec::new(),
            resumo: Resumo::default(),
        };
        c.resumo.livre_e_estimativa = false;
        let l = cartoes(&c, hoje())
            .into_iter()
            .find(|k| k.id == "livre")
            .unwrap();
        assert_eq!(l.selo, None);
        c.resumo.livre_e_estimativa = true;
        let l = cartoes(&c, hoje())
            .into_iter()
            .find(|k| k.id == "livre")
            .unwrap();
        assert_eq!(l.selo, Some("estimativa"));
    }

    #[test]
    fn saldo_passou_de_um_dia_e_desatualizado() {
        let s = |q: &str| Saldo {
            conta: "X".into(),
            valor: Centavos(1),
            quando: q.into(),
        };
        assert!(!saldo_desatualizado(&s("2026-11-16T07:00:00Z"), hoje()));
        assert!(!saldo_desatualizado(&s("2026-11-15T23:00:00Z"), hoje()));
        assert!(saldo_desatualizado(&s("2026-11-14T23:00:00Z"), hoje()));
        assert!(saldo_desatualizado(&s("lixo"), hoje()));
    }

    #[test]
    fn mes_futuro_distante_e_projecao() {
        let mut n = Navegador::novo(hoje());
        n.trocar_modo(ModoDoPeriodo::MesCivil);
        n.ir_para(Competencia { ano: 2050, mes: 1 }).unwrap();
        let c = controle_do(&n);
        assert!(n.e_projecao());
        let (rotulo, _) = origem(&c.linhas[0], true);
        assert_eq!(rotulo, "Projeção");
        let (rotulo, _) = origem(&c.linhas[0], false);
        assert_ne!(rotulo, "Projeção");
    }

    #[test]
    fn disponibilidade_das_acoes() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let atraso = c
            .linhas
            .iter()
            .find(|o| o.estado == EstadoDaOcorrencia::EmAtraso && !o.sugestao_de_conciliacao)
            .expect("em atraso");
        let sem = ContextoDaLinha::default();
        assert!(disponibilidade(AcaoDoControle::Editar, &sem).is_err());
        let ctx = ContextoDaLinha {
            linha: Some(atraso),
            ..Default::default()
        };
        assert!(disponibilidade(AcaoDoControle::Editar, &ctx).is_ok());
        assert!(disponibilidade(AcaoDoControle::Pausar, &ctx).is_ok());
        assert!(disponibilidade(AcaoDoControle::Retomar, &ctx).is_err());
        assert!(disponibilidade(AcaoDoControle::ConfirmarConciliacao, &ctx).is_err());
        let sug = c
            .linhas
            .iter()
            .find(|o| o.sugestao_de_conciliacao)
            .expect("sugestão");
        let ctx = ContextoDaLinha {
            linha: Some(sug),
            ..Default::default()
        };
        assert!(disponibilidade(AcaoDoControle::ConfirmarConciliacao, &ctx).is_ok());
        let ctx = ContextoDaLinha {
            linha: Some(sug),
            primeira_do_grupo: true,
            ..Default::default()
        };
        assert_eq!(
            disponibilidade(AcaoDoControle::Subir, &ctx).unwrap_err(),
            "Já é a primeira do grupo."
        );
        assert!(disponibilidade(AcaoDoControle::Propostas, &sem).is_ok());
        assert!(disponibilidade(AcaoDoControle::Restaurar, &sem).is_err());
    }

    #[test]
    fn acoes_tem_codigo_e_atalho_unicos_e_icone() {
        let mut codigos: Vec<_> = ACOES_DO_CONTROLE.iter().map(|d| d.codigo).collect();
        codigos.sort_unstable();
        codigos.dedup();
        assert_eq!(codigos.len(), ACOES_DO_CONTROLE.len());
        let mut atalhos: Vec<_> = ACOES_DO_CONTROLE
            .iter()
            .map(|d| {
                (
                    d.atalho.ctrl,
                    d.atalho.shift,
                    d.atalho.alt,
                    format!("{:?}", d.atalho.tecla),
                )
            })
            .collect();
        atalhos.sort();
        atalhos.dedup();
        assert_eq!(atalhos.len(), ACOES_DO_CONTROLE.len());
        let mut keytips: Vec<_> = ACOES_DO_CONTROLE.iter().map(|d| d.keytip).collect();
        keytips.sort_unstable();
        keytips.dedup();
        assert_eq!(keytips.len(), ACOES_DO_CONTROLE.len());
        for d in &ACOES_DO_CONTROLE {
            assert!(crate::icones::caractere(d.icone).is_some(), "{}", d.icone);
            assert_eq!(acao_do_codigo(d.codigo), Some(d.id));
        }
    }

    fn alvo() -> AlvoDoDialogo {
        AlvoDoDialogo {
            recorrente: "rec_internet".into(),
            nome: "Internet fibra".into(),
            versao_base: 1,
            ocorrencia: Some(IdOcorrencia {
                recorrente: "rec_internet".into(),
                competencia: Competencia { ano: 2026, mes: 11 },
                parcela: None,
            }),
        }
    }

    fn atual() -> AtualDaVersao {
        AtualDaVersao {
            valor: Some(Centavos(11_990)),
            conta: "Nubank".into(),
            centro: "Casa".into(),
            certeza: sbf_planejamento::Certeza::Confirmado,
        }
    }

    #[test]
    fn editar_escolhe_o_escopo_e_nao_mistura() {
        let comp = Competencia { ano: 2026, mes: 12 };
        let f = |escopo, valor: &str, conta: &str| Formulario::Editar {
            escopo,
            valor: valor.into(),
            conta: conta.into(),
            centro: "Casa".into(),
            estimado: false,
        };
        let a = montar_alteracao(
            &alvo(),
            comp,
            &f(EscopoEscolhido::SoEsteMes, "130,00", "Nubank"),
            &atual(),
        )
        .unwrap();
        assert!(matches!(
            a,
            Alteracao::Editar { escopo: sbf_planejamento::Escopo::SoEsteMes(c), .. } if c == comp
        ));
        let a = montar_alteracao(
            &alvo(),
            comp,
            &f(EscopoEscolhido::DaquiEmDiante, "130,00", "CX-0630"),
            &atual(),
        )
        .unwrap();
        match a {
            Alteracao::Editar {
                escopo,
                mudanca,
                versao_base,
                ..
            } => {
                assert_eq!(escopo, sbf_planejamento::Escopo::DaquiEmDiante(comp));
                assert_eq!(mudanca.valor, Some(Some(Centavos(13_000))));
                assert_eq!(mudanca.conta, Some(Some("CX-0630".into())));
                assert_eq!(versao_base, 1);
            }
            outra => panic!("{outra:?}"),
        }
        // Só este mês com conta nova: recusa.
        assert!(
            montar_alteracao(
                &alvo(),
                comp,
                &f(EscopoEscolhido::SoEsteMes, "130,00", "CX-0630"),
                &atual()
            )
            .is_err()
        );
        // Nada mudou.
        assert_eq!(
            montar_alteracao(
                &alvo(),
                comp,
                &f(EscopoEscolhido::DaquiEmDiante, "119,90", "Nubank"),
                &atual()
            )
            .unwrap_err(),
            "Nada mudou."
        );
        assert!(
            montar_alteracao(
                &alvo(),
                comp,
                &f(EscopoEscolhido::DaquiEmDiante, "abc", "Nubank"),
                &atual()
            )
            .is_err()
        );
    }

    #[test]
    fn excluir_exige_motivo_e_confirmacao() {
        let comp = Competencia { ano: 2026, mes: 11 };
        let f = |motivo: &str, confirmou| Formulario::Excluir {
            motivo: motivo.into(),
            confirmou,
        };
        assert!(montar_alteracao(&alvo(), comp, &f("", true), &atual()).is_err());
        assert!(montar_alteracao(&alvo(), comp, &f("duplicado", false), &atual()).is_err());
        assert!(matches!(
            montar_alteracao(&alvo(), comp, &f("duplicado", true), &atual()).unwrap(),
            Alteracao::Excluir { .. }
        ));
    }

    #[test]
    fn pausar_valida_os_meses() {
        let comp = Competencia { ano: 2026, mes: 12 };
        let f = |desde: &str, ate: &str| Formulario::Pausar {
            desde: desde.into(),
            ate: ate.into(),
            motivo: "viagem".into(),
        };
        let a = montar_alteracao(&alvo(), comp, &f("12/2026", "03/2027"), &atual()).unwrap();
        assert!(
            matches!(a, Alteracao::Pausar { ate: Some(c), .. } if c == Competencia { ano: 2027, mes: 3 })
        );
        assert!(montar_alteracao(&alvo(), comp, &f("12/2026", "12/2026"), &atual()).is_err());
        assert!(montar_alteracao(&alvo(), comp, &f("2026-12", ""), &atual()).is_ok());
        assert!(montar_alteracao(&alvo(), comp, &f("13/2026", ""), &atual()).is_err());
    }

    #[test]
    fn informar_pago_e_adiar_viram_excecao() {
        let comp = Competencia { ano: 2026, mes: 11 };
        let a = montar_alteracao(
            &alvo(),
            comp,
            &Formulario::InformarPago {
                valor: "119,90".into(),
                data: "15/11/2026".into(),
            },
            &atual(),
        )
        .unwrap();
        assert!(matches!(
            a,
            Alteracao::Excecao {
                excecao: Excecao::PagoInformado {
                    valor: Centavos(11_990),
                    data: Some(_)
                },
                ..
            }
        ));
        let a = montar_alteracao(
            &alvo(),
            comp,
            &Formulario::Adiar {
                nova_data: String::new(),
            },
            &atual(),
        )
        .unwrap();
        assert!(matches!(
            a,
            Alteracao::Excecao {
                excecao: Excecao::Adiar { nova_data: None },
                ..
            }
        ));
        assert!(
            montar_alteracao(
                &alvo(),
                comp,
                &Formulario::Adiar {
                    nova_data: "31/02/2026x".into()
                },
                &atual()
            )
            .is_err()
        );
    }

    #[test]
    fn trocar_prioridade_inverte_as_posicoes() {
        let s = PlanejamentoFicticio::novo(hoje());
        let a = s.recorrente("rec_aluguel").unwrap().unwrap();
        let b = s.recorrente("rec_condominio").unwrap().unwrap();
        let [x, y] = trocar_prioridade(&a, &b);
        match (x, y) {
            (Alteracao::Mover { posicao: pa, .. }, Alteracao::Mover { posicao: pb, .. }) => {
                assert_eq!(pa, b.posicao);
                assert_eq!(pb, a.posicao);
            }
            _ => panic!("esperava dois Mover"),
        }
    }

    #[test]
    fn descrever_em_portugues() {
        let s = PlanejamentoFicticio::novo(hoje());
        let nome = |id: &str| s.recorrente(id).unwrap().map_or(id.to_owned(), |r| r.nome);
        let t = descrever(
            &Alteracao::Pausar {
                recorrente: "rec_streaming".into(),
                desde: Competencia { ano: 2026, mes: 12 },
                ate: Some(Competencia { ano: 2027, mes: 3 }),
                motivo: "viagem".into(),
                versao_base: 1,
            },
            &nome,
        );
        assert_eq!(
            t,
            "Pausar \u{201c}Streaming de vídeo\u{201d} de 12/2026 até 03/2027 (exclusivo) (viagem)."
        );
        for p in s.propostas(false).unwrap() {
            let t = descrever(&p.alteracao, &nome);
            assert!(t.ends_with('.') && !t.contains("rec_"), "{t}");
        }
    }

    #[test]
    fn compra_no_cartao_fica_fora_das_saidas_e_vai_a_parte() {
        let n = Navegador::novo(hoje());
        let c = controle_do(&n);
        let g = montar_grade(&c);
        assert!(c.resumo.compras_no_cartao > Centavos::ZERO);
        assert_eq!(g.total.no_cartao, c.resumo.compras_no_cartao);
        assert_eq!(g.total.saidas, c.resumo.saidas_de_caixa_previstas);
        let pluggy = g
            .grupos
            .iter()
            .find(|x| x.nome == "Achados na Pluggy")
            .expect("grupo");
        let subs: Vec<_> = pluggy
            .subgrupos
            .iter()
            .filter_map(|s| s.nome.as_deref())
            .collect();
        assert!(subs.contains(&"No cartão") && subs.contains(&"Débito em conta"));
        assert!(
            pluggy
                .total
                .partes_do_previsto()
                .iter()
                .any(|(n, _)| *n == "No cartão")
        );
    }
}
