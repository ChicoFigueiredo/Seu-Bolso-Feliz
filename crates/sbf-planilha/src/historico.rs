//! Importação histórica: todas as abas mensais `AAAA.MM` de uma planilha, de uma vez.
//!
//! Importar as abas uma a uma, em ordem, faz cada aba antiga criar compromissos "sem fim"
//! que projetam até hoje. Aqui o arquivo é lido inteiro antes de gravar qualquer coisa e cada
//! compromisso (casado pelo mesmo critério do importador de uma aba: nome normalizado,
//! preferindo o mesmo grupo) ganha uma vigência:
//!
//! - começa no PRIMEIRO mês em que aparece e vai até o ÚLTIMO;
//! - se não aparece na aba mais recente lida, é encerrado com `Alteracao::Cancelar`
//!   (`desde` = mês seguinte ao último visto, `comprovado = false`, motivo "não aparece
//!   mais na planilha desde AAAA.MM", autor `Importacao`);
//! - mês no meio em que não aparece vira `Excecao::Dispensar` ("não houve");
//! - o valor muda por versão (`Alteracao::Editar` daqui em diante) e o pago de cada mês
//!   vira `Excecao::PagoInformado`;
//! - grupo ausente (layout antigo) herda o do mesmo compromisso em meses posteriores; sem
//!   nenhum, vai para [`GRUPO_ANTIGO`].
//!
//! Por fim a aba mais recente passa pelo importador normal (composição do salário, adiados
//! e o que o mapa pedir), então a projeção dali em diante é a de importar só ela.
//!
//! Aba sem linha lida (layout que o leitor não entende) não conta como mês observado: não
//! cria buraco nem encerra ninguém. É idempotente: o que o banco já tem não é regravado.
//! Nenhum nome, valor ou descrição vai a mensagem de erro: só nomes de aba e contagens.

use std::collections::HashSet;
use std::path::Path;

use sbf_planejamento::{
    Alteracao, Autor, Competencia, ErroDoPlanejamento, Escopo, Excecao, ExcecaoGravada, Fonte,
    Frequencia, Grupo, IdOcorrencia, MudancaDeVersao, NovoRecorrente, OrigemImportada,
    Planejamento, Prazo, Recorrente, Versao,
};

use crate::Resultado;
use crate::importacao::{self, Candidato, PagoDoMes};
use crate::leitura::{self, AbaLida};
use crate::mapa::Mapa;
use crate::texto::normalizar;

/// O grupo dos compromissos que só aparecem em abas sem a coluna de grupo.
pub const GRUPO_ANTIGO: &str = "Planilha antiga";

/// Quais abas ler: de/até inclusive. Sem limite, todas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Janela {
    pub de: Option<Competencia>,
    pub ate: Option<Competencia>,
}

impl Janela {
    fn contem(&self, c: Competencia) -> bool {
        self.de.is_none_or(|d| c >= d) && self.ate.is_none_or(|a| c <= a)
    }
}

/// O que aconteceu (ou aconteceria, na prévia) com uma aba.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatorioDaAba {
    pub aba: String,
    /// `Some("layout")` quando a aba não tem linha lida; `Some("ilegível")` quando falhou.
    pub nao_lida: Option<String>,
    pub lidas: u32,
    /// Compromissos que começam nesta aba (e ainda não existiam).
    pub novos: u32,
    /// Compromissos cujo último mês visto é este e que terminam aqui.
    pub encerrados: u32,
    /// Meses "não houve" (dispensas) nesta aba.
    pub excecoes: u32,
    pub pagos: u32,
    /// Versões novas de valor (ou de data, conta, certeza) nesta aba.
    pub versoes: u32,
    /// Decisões que o importador não tocou (valor decidido por você, cancelamento seu...).
    pub conflitos: u32,
    pub falhas: u32,
}

impl RelatorioDaAba {
    fn nova(aba: &str) -> RelatorioDaAba {
        RelatorioDaAba {
            aba: aba.to_owned(),
            ..RelatorioDaAba::default()
        }
    }

    /// Só contagens, uma linha.
    pub fn linha(&self) -> String {
        match &self.nao_lida {
            Some(m) => format!("{}: não lida ({m})", self.aba),
            None => format!(
                "{}: lidas={} novos={} encerrados={} excecoes={} pagos={} versoes={} conflitos={} falhas={}",
                self.aba,
                self.lidas,
                self.novos,
                self.encerrados,
                self.excecoes,
                self.pagos,
                self.versoes,
                self.conflitos,
                self.falhas
            ),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatorioDoHistorico {
    /// Uma por aba mensal do arquivo, em ordem cronológica (as não lidas também).
    pub abas: Vec<RelatorioDaAba>,
    pub grupos: u32,
    pub composicoes: u32,
    pub adiadas: u32,
    /// `false` = prévia, nada foi gravado.
    pub confirmado: bool,
    /// Linha a linha, com nomes: só para o terminal, nunca para log.
    pub detalhes: Vec<String>,
    /// `aba: motivo genérico`, sem nome de compromisso.
    pub falhas: Vec<String>,
}

impl RelatorioDoHistorico {
    /// A soma das abas.
    pub fn total(&self) -> RelatorioDaAba {
        let mut t = RelatorioDaAba::nova("total");
        for a in &self.abas {
            t.lidas += a.lidas;
            t.novos += a.novos;
            t.encerrados += a.encerrados;
            t.excecoes += a.excecoes;
            t.pagos += a.pagos;
            t.versoes += a.versoes;
            t.conflitos += a.conflitos;
            t.falhas += a.falhas;
        }
        t
    }
}

/// Uma aba do arquivo, lida ou não.
#[derive(Debug)]
pub struct EntradaDeAba {
    pub nome: String,
    pub competencia: Competencia,
    /// `Err` leva só o motivo genérico.
    pub aba: Result<AbaLida, String>,
}

// ---------------------------------------------------------------------------------------
// Interno
// ---------------------------------------------------------------------------------------

/// O que decide se uma versão mudou. Só isto vira versão nova.
#[derive(Debug, Clone, PartialEq)]
struct Tupla {
    valor: Option<sbf_planejamento::Centavos>,
    certeza: sbf_planejamento::Certeza,
    frequencia: Frequencia,
    conta: Option<String>,
    centro: Option<String>,
    fornecedor: Option<String>,
}

fn tupla_de(v: &Versao) -> Tupla {
    Tupla {
        valor: v.valor,
        certeza: v.certeza,
        frequencia: v.frequencia,
        conta: v.conta.clone(),
        centro: v.centro.clone(),
        fornecedor: v.fornecedor.clone(),
    }
}

fn aplicar_tupla(v: &mut Versao, t: &Tupla) {
    v.valor = t.valor;
    v.certeza = t.certeza;
    v.frequencia = t.frequencia;
    v.conta.clone_from(&t.conta);
    v.centro.clone_from(&t.centro);
    v.fornecedor.clone_from(&t.fornecedor);
    v.fonte = Fonte::Planilha;
}

/// Os campos de `novo` que diferem de `atual` (e a fonte).
fn mudanca_entre(atual: &Tupla, novo: &Tupla) -> MudancaDeVersao {
    MudancaDeVersao {
        valor: (atual.valor != novo.valor).then_some(novo.valor),
        certeza: (atual.certeza != novo.certeza).then_some(novo.certeza),
        fonte: Some(Fonte::Planilha),
        frequencia: (atual.frequencia != novo.frequencia).then_some(novo.frequencia),
        conta: (atual.conta != novo.conta).then(|| novo.conta.clone()),
        centro: (atual.centro != novo.centro).then(|| novo.centro.clone()),
        fornecedor: (atual.fornecedor != novo.fornecedor).then(|| novo.fornecedor.clone()),
        ..MudancaDeVersao::default()
    }
}

fn mudanca_completa(novo: &Tupla) -> MudancaDeVersao {
    MudancaDeVersao {
        valor: Some(novo.valor),
        certeza: Some(novo.certeza),
        fonte: Some(Fonte::Planilha),
        frequencia: Some(novo.frequencia),
        conta: Some(novo.conta.clone()),
        centro: Some(novo.centro.clone()),
        fornecedor: Some(novo.fornecedor.clone()),
        ..MudancaDeVersao::default()
    }
}

struct Observada {
    entrada: usize,
    competencia: Competencia,
    aba: AbaLida,
}

struct Aparicao {
    /// Índice em `observadas`.
    obs: usize,
    cand: Candidato,
}

struct Compromisso {
    chave: String,
    grupo_ultimo: String,
    aparicoes: Vec<Aparicao>,
}

enum Acao {
    Grupo(Grupo),
    Criar {
        novo: NovoRecorrente,
        origem: OrigemImportada,
    },
    Editar {
        desde: Competencia,
        mudanca: MudancaDeVersao,
    },
    Dispensar {
        competencia: Competencia,
        motivo: String,
    },
    Pago {
        pago: PagoDoMes,
        prazo: Prazo,
    },
    Cancelar {
        desde: Competencia,
        motivo: String,
    },
}

struct Passo {
    compromisso: usize,
    entrada: usize,
    nome: String,
    acao: Acao,
}

impl Passo {
    fn contar(&self, r: &mut RelatorioDaAba) {
        match self.acao {
            Acao::Grupo(_) => {}
            Acao::Criar { .. } => r.novos += 1,
            Acao::Editar { .. } => r.versoes += 1,
            Acao::Dispensar { .. } => r.excecoes += 1,
            Acao::Pago { .. } => r.pagos += 1,
            Acao::Cancelar { .. } => r.encerrados += 1,
        }
    }

    fn descrever(&self) -> String {
        match &self.acao {
            Acao::Grupo(g) => format!("grupo   {}", g.nome),
            Acao::Criar { novo, .. } => format!(
                "novo    {} | {} | desde {}",
                novo.nome, novo.grupo, novo.primeira_versao.desde
            ),
            Acao::Editar { desde, .. } => format!("versao  {} | desde {desde}", self.nome),
            Acao::Dispensar { competencia, .. } => {
                format!("nao houve {} | {competencia}", self.nome)
            }
            Acao::Pago { pago, .. } => format!(
                "pago    {} | {} | {} centavos",
                self.nome, pago.competencia, pago.valor.0
            ),
            Acao::Cancelar { desde, .. } => format!("encerra {} | desde {desde}", self.nome),
        }
    }
}

enum Alvo {
    Novo,
    Existente(usize),
    /// A origem aponta para um recorrente excluído: não mexe.
    Ignorado,
}

/// O plano completo, calculado sem gravar nada.
pub struct Plano {
    sha: String,
    base: Vec<RelatorioDaAba>,
    observadas: Vec<Observada>,
    passos: Vec<Passo>,
    /// Por compromisso: o id e a versão do registro, se já existe no banco.
    do_banco: Vec<Option<(String, i64)>>,
    /// Compromissos casados com um recorrente de outro nome (renomeado no app): o importador
    /// normal, que só olha o nome de hoje, não pode criá-los de novo.
    por_apelido: HashSet<String>,
}

impl std::fmt::Debug for Plano {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plano")
            .field("abas", &self.base.len())
            .field("passos", &self.passos.len())
            .finish_non_exhaustive()
    }
}

fn rotulo(e: &ErroDoPlanejamento) -> &'static str {
    match e {
        ErroDoPlanejamento::VersaoVelha { .. } => "versão do registro mudou",
        ErroDoPlanejamento::NaoEncontrado(_) => "registro não encontrado",
        ErroDoPlanejamento::Invalida(_) => "alteração recusada",
        ErroDoPlanejamento::Banco(_) => "erro de banco",
    }
}

/// O que se sabe de um recorrente do banco para casá-lo com a planilha.
struct Conhecido {
    /// Nasceu da planilha (a primeira versão vem dela): só estes o histórico encerra,
    /// dispensa ou estende para trás. Pluggy, achados, MCP e o que você criou ficam como estão.
    da_planilha: bool,
    /// Os nomes normalizados que ele já teve: o atual, o da criação e os das renomeações.
    nomes: Vec<String>,
}

fn conhecer(plan: &dyn Planejamento, r: &Recorrente) -> Resultado<Conhecido> {
    let mut nomes = vec![normalizar(&r.nome)];
    for e in plan.historico(Some(&r.id), 1000)? {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&e.alteracao) else {
            continue;
        };
        let nome = match v.get("tipo").and_then(|t| t.as_str()) {
            Some("criar_recorrente") => v.pointer("/recorrente/nome"),
            Some("renomear") => v.get("nome"),
            _ => None,
        };
        if let Some(n) = nome.and_then(|n| n.as_str()) {
            let n = normalizar(n);
            if !nomes.contains(&n) {
                nomes.push(n);
            }
        }
    }
    let da_planilha = r
        .versoes
        .iter()
        .min_by_key(|v| v.desde)
        .is_some_and(|v| v.fonte == Fonte::Planilha);
    Ok(Conhecido { da_planilha, nomes })
}

/// O recorrente do banco que casa com o compromisso: um dos nomes que ele já teve igual ao
/// da planilha, preferindo o mesmo grupo, cada um uma vez só. `da_planilha` separa os que
/// nasceram dela dos demais.
fn casar_no_banco(
    existentes: &[Recorrente],
    conhecidos: &[Conhecido],
    chave: &str,
    grupo: &str,
    usados: &HashSet<String>,
    da_planilha: bool,
) -> Option<usize> {
    let grupo = normalizar(grupo);
    let mesmos = |i: &usize| {
        !usados.contains(&existentes[*i].id)
            && conhecidos[*i].da_planilha == da_planilha
            && conhecidos[*i].nomes.iter().any(|n| n == chave)
    };
    (0..existentes.len())
        .filter(mesmos)
        .find(|i| normalizar(&existentes[*i].grupo) == grupo)
        .or_else(|| (0..existentes.len()).find(mesmos))
}

fn ativa<'a>(
    excecoes: &'a [ExcecaoGravada],
    competencia: Competencia,
    quer: impl Fn(&Excecao) -> bool + 'a,
) -> bool {
    excecoes
        .iter()
        .any(|e| !e.desfeita && e.ocorrencia.competencia == competencia && quer(&e.excecao))
}

/// Lê as abas e calcula tudo o que gravar. Não escreve no `plan`.
pub fn planejar(
    entradas: Vec<EntradaDeAba>,
    sha: &str,
    plan: &dyn Planejamento,
    mapa: &Mapa,
) -> Resultado<Plano> {
    let mut base: Vec<RelatorioDaAba> = entradas
        .iter()
        .map(|e| RelatorioDaAba::nova(&e.nome))
        .collect();
    let mut observadas: Vec<Observada> = Vec::new();
    for (i, e) in entradas.into_iter().enumerate() {
        match e.aba {
            Err(motivo) => base[i].nao_lida = Some(motivo),
            Ok(aba) if aba.linhas.is_empty() => base[i].nao_lida = Some("layout".to_owned()),
            Ok(aba) => {
                base[i].lidas = aba.linhas.len() as u32;
                observadas.push(Observada {
                    entrada: i,
                    competencia: e.competencia,
                    aba,
                });
            }
        }
    }
    let mut plano = Plano {
        sha: sha.to_owned(),
        base,
        observadas: Vec::new(),
        passos: Vec::new(),
        do_banco: Vec::new(),
        por_apelido: HashSet::new(),
    };
    if observadas.is_empty() {
        return Ok(plano);
    }

    // 1) Os candidatos de cada aba, casados em compromissos pelo nome.
    let ultima_obs = observadas.len() - 1;
    let vazio = Mapa::vazio();
    let mut compromissos: Vec<Compromisso> = Vec::new();
    let mut ordem_da_ultima: Vec<usize> = Vec::new();
    for (j, o) in observadas.iter().enumerate() {
        // O mapa vale para a aba mais recente (parcelas, atrasos, naturezas); o salário
        // usa o mapa em todas, para o dia não variar de um mês para outro.
        let mapa_das_linhas = if j == ultima_obs { mapa } else { &vazio };
        let mut descartados = Vec::new();
        let mut candidatos: Vec<Candidato> = Vec::new();
        if let Some(c) =
            importacao::candidato_do_salario(&o.aba, mapa, o.competencia, &mut descartados)
        {
            candidatos.push(c);
        }
        for (i, l) in o.aba.linhas.iter().enumerate() {
            candidatos.push(importacao::candidato_da_linha(
                &o.aba,
                l,
                i as i64 + 1,
                mapa_das_linhas,
                o.competencia,
                &mut descartados,
            ));
        }
        let mut usados: HashSet<usize> = HashSet::new();
        for cand in candidatos {
            let chave = normalizar(&cand.nome);
            let grupo = normalizar(&cand.grupo);
            let livres = |i: &usize| !usados.contains(i) && compromissos[*i].chave == chave;
            let achado = (0..compromissos.len())
                .filter(livres)
                .find(|i| normalizar(&compromissos[*i].grupo_ultimo) == grupo)
                .or_else(|| (0..compromissos.len()).find(livres));
            let ci = achado.unwrap_or_else(|| {
                compromissos.push(Compromisso {
                    chave,
                    grupo_ultimo: String::new(),
                    aparicoes: Vec::new(),
                });
                compromissos.len() - 1
            });
            usados.insert(ci);
            if j == ultima_obs {
                ordem_da_ultima.push(ci);
            }
            if !cand.grupo.trim().is_empty() {
                compromissos[ci].grupo_ultimo.clone_from(&cand.grupo);
            }
            compromissos[ci].aparicoes.push(Aparicao { obs: j, cand });
        }
    }

    // 2) Grupo final e centro/fornecedor (do mapa, que só vale para a aba mais recente)
    // iguais em todos os meses do compromisso.
    let mut grupos_finais: Vec<String> = Vec::with_capacity(compromissos.len());
    for c in &mut compromissos {
        let grupo = c
            .aparicoes
            .iter()
            .rev()
            .map(|a| a.cand.grupo.trim())
            .find(|g| !g.is_empty())
            .unwrap_or(GRUPO_ANTIGO)
            .to_owned();
        if let Some(u) = c.aparicoes.last() {
            let (centro, fornecedor) = (
                u.cand.versao.centro.clone(),
                u.cand.versao.fornecedor.clone(),
            );
            for a in &mut c.aparicoes {
                a.cand.versao.centro.clone_from(&centro);
                a.cand.versao.fornecedor.clone_from(&fornecedor);
            }
        }
        grupos_finais.push(grupo);
    }

    // 3) Casa com o que o banco já tem: primeiro pela origem, depois pelo nome. Quem está
    // na aba mais recente casa primeiro (foi dela que o banco costuma ter vindo).
    let existentes = plan.recorrentes(false)?;
    let conhecidos: Vec<Conhecido> = existentes
        .iter()
        .map(|r| conhecer(plan, r))
        .collect::<Resultado<_>>()?;
    let mut por_apelido: HashSet<String> = HashSet::new();
    let mut usados_banco: HashSet<String> = HashSet::new();
    let mut alvos: Vec<Alvo> = (0..compromissos.len()).map(|_| Alvo::Novo).collect();
    let mut ordem: Vec<usize> = (0..compromissos.len()).collect();
    ordem.sort_by_key(|&ci| {
        let na_ultima = compromissos[ci]
            .aparicoes
            .last()
            .is_some_and(|a| a.obs == ultima_obs);
        (!na_ultima, ci)
    });
    for ci in ordem {
        let Some(primeira) = compromissos[ci].aparicoes.first() else {
            continue;
        };
        let origem = primeira
            .cand
            .origem
            .clone()
            .unwrap_or_else(|| OrigemImportada {
                arquivo_sha256: sha.to_owned(),
                aba: plano.base[observadas[primeira.obs].entrada].aba.clone(),
                linha: primeira.cand.linha,
            });
        if let Some(id) = plan.origem_importada(&origem)? {
            match existentes.iter().position(|r| r.id == id) {
                Some(i) => {
                    usados_banco.insert(id);
                    alvos[ci] = Alvo::Existente(i);
                    if normalizar(&existentes[i].nome) != compromissos[ci].chave {
                        por_apelido.insert(compromissos[ci].chave.clone());
                    }
                }
                None => alvos[ci] = Alvo::Ignorado,
            }
            continue;
        }
        // Primeiro os que nasceram da planilha (mesmo renomeados no app). Um que não nasceu
        // dela só casa se o compromisso está na aba mais recente (como ao importar só ela)
        // e então só entra o mês mais recente: nada de encerrar ou estender o passado dele.
        let na_ultima = compromissos[ci]
            .aparicoes
            .last()
            .is_some_and(|a| a.obs == ultima_obs);
        let achado = casar_no_banco(
            &existentes,
            &conhecidos,
            &compromissos[ci].chave,
            &grupos_finais[ci],
            &usados_banco,
            true,
        )
        .map(|i| (i, true))
        .or_else(|| {
            if !na_ultima {
                return None;
            }
            casar_no_banco(
                &existentes,
                &conhecidos,
                &compromissos[ci].chave,
                &grupos_finais[ci],
                &usados_banco,
                false,
            )
            .map(|i| (i, false))
        });
        if let Some((i, da_planilha)) = achado {
            usados_banco.insert(existentes[i].id.clone());
            alvos[ci] = Alvo::Existente(i);
            if normalizar(&existentes[i].nome) != compromissos[ci].chave {
                por_apelido.insert(compromissos[ci].chave.clone());
            }
            if !da_planilha {
                let n = compromissos[ci].aparicoes.len();
                compromissos[ci].aparicoes.drain(..n - 1);
            }
        }
    }

    // 4) Os grupos: primeiro os da aba mais recente, na ordem dela (as posições ficam
    // iguais às de importar só ela); depois os que só existem nas abas antigas.
    let grupos_do_banco = plan.grupos()?;
    let mut proxima = grupos_do_banco
        .iter()
        .map(|g| g.posicao + 1)
        .max()
        .unwrap_or(0);
    let mut vistos: HashSet<String> = grupos_do_banco
        .iter()
        .map(|g| normalizar(&g.nome))
        .collect();
    let entrada_da_ultima = observadas[ultima_obs].entrada;
    let mut passos: Vec<Passo> = Vec::new();
    let em_ordem = ordem_da_ultima.iter().copied().chain(0..compromissos.len());
    for ci in em_ordem {
        if vistos.insert(normalizar(&grupos_finais[ci])) {
            passos.push(Passo {
                compromisso: usize::MAX,
                entrada: entrada_da_ultima,
                nome: String::new(),
                acao: Acao::Grupo(Grupo {
                    nome: grupos_finais[ci].clone(),
                    posicao: proxima,
                }),
            });
            proxima += 1;
        }
    }

    // 5) Por compromisso: criação, versões, dispensas, pagos e encerramento.
    for (ci, c) in compromissos.iter().enumerate() {
        let (Some(primeira), Some(ultima)) = (c.aparicoes.first(), c.aparicoes.last()) else {
            continue;
        };
        let rec_banco: Option<&Recorrente> = match alvos[ci] {
            Alvo::Ignorado => {
                let e = observadas[ultima.obs].entrada;
                plano.base[e].conflitos += 1;
                continue;
            }
            Alvo::Novo => None,
            Alvo::Existente(i) => Some(&existentes[i]),
        };
        let nome = ultima.cand.nome.clone();
        let entrada_de = |obs: usize| observadas[obs].entrada;
        let mut passo = |obs: usize, acao: Acao| {
            passos.push(Passo {
                compromisso: ci,
                entrada: entrada_de(obs),
                nome: nome.clone(),
                acao,
            });
        };

        // Os pontos de mudança: o primeiro mês e cada mês em que a tupla muda. Um valor que
        // não deu para ler no meio do caminho (erro de fórmula) não apaga o anterior; na
        // última aparição vale o que a aba mais recente diz, como ao importar só ela.
        struct Ponto {
            obs: usize,
            desde: Competencia,
            tupla: Tupla,
        }
        let mut pontos: Vec<Ponto> = Vec::new();
        let mut anterior: Option<Tupla> = None;
        for (k, a) in c.aparicoes.iter().enumerate() {
            let mut t = tupla_de(&a.cand.versao);
            if t.valor.is_none()
                && k + 1 < c.aparicoes.len()
                && let Some(p) = &anterior
            {
                t.valor = p.valor;
                t.certeza = p.certeza;
            }
            let mut desde = a.cand.versao.desde;
            if k > 0 {
                desde = desde.max(observadas[a.obs].competencia);
                if let Some(p) = pontos.last() {
                    desde = desde.max(p.desde);
                }
            }
            if k == 0 || anterior.as_ref() != Some(&t) {
                pontos.push(Ponto {
                    obs: a.obs,
                    desde,
                    tupla: t.clone(),
                });
            }
            anterior = Some(t);
        }

        let excecoes_banco: Vec<ExcecaoGravada> = match rec_banco {
            Some(r) => plan.excecoes_do_recorrente(&r.id)?,
            None => Vec::new(),
        };
        let prazo = rec_banco.map_or(ultima.cand.prazo, |r| r.prazo);

        // Criação (com a primeira versão) ou o que o banco já tem de versões.
        let mut sim: Vec<Versao> =
            match rec_banco {
                Some(r) => r.versoes.clone(),
                None => {
                    let mut v = primeira.cand.versao.clone();
                    v.desde = pontos[0].desde;
                    passo(
                        primeira.obs,
                        Acao::Criar {
                            novo: NovoRecorrente {
                                nome: ultima.cand.nome.clone(),
                                grupo: grupos_finais[ci].clone(),
                                subgrupo: None,
                                posicao: ultima.cand.posicao,
                                natureza: ultima.cand.natureza,
                                prazo: ultima.cand.prazo,
                                primeira_versao: v.clone(),
                            },
                            origem: primeira.cand.origem.clone().unwrap_or_else(|| {
                                OrigemImportada {
                                    arquivo_sha256: sha.to_owned(),
                                    aba: plano.base[entrada_de(primeira.obs)].aba.clone(),
                                    linha: primeira.cand.linha,
                                }
                            }),
                        },
                    );
                    vec![v]
                }
            };
        let a_partir = usize::from(rec_banco.is_none());
        for p in pontos.iter().skip(a_partir) {
            let vigente = sim
                .iter()
                .enumerate()
                .filter(|(_, v)| v.desde <= p.desde)
                .max_by_key(|(_, v)| v.desde)
                .map(|(i, _)| i);
            let mudanca = match vigente {
                // Antes da primeira versão que o banco tem: estende a vigência para trás.
                None => {
                    let Some(base_v) = sim.iter().min_by_key(|v| v.desde).cloned() else {
                        continue;
                    };
                    let mut nova = base_v;
                    aplicar_tupla(&mut nova, &p.tupla);
                    nova.desde = p.desde;
                    sim.push(nova);
                    mudanca_completa(&p.tupla)
                }
                Some(i) => {
                    let atual = tupla_de(&sim[i]);
                    if atual == p.tupla {
                        continue;
                    }
                    if sim[i].fonte == Fonte::Usuario {
                        plano.base[entrada_de(p.obs)].conflitos += 1;
                        continue;
                    }
                    let m = mudanca_entre(&atual, &p.tupla);
                    if sim[i].desde == p.desde {
                        aplicar_tupla(&mut sim[i], &p.tupla);
                    } else {
                        let mut nova = sim[i].clone();
                        aplicar_tupla(&mut nova, &p.tupla);
                        nova.desde = p.desde;
                        sim.push(nova);
                    }
                    m
                }
            };
            sim.sort_by_key(|v| v.desde);
            passo(
                p.obs,
                Acao::Editar {
                    desde: p.desde,
                    mudanca,
                },
            );
        }

        // Meses do meio em que o compromisso não aparece: "não houve".
        let presentes: HashSet<usize> = c.aparicoes.iter().map(|a| a.obs).collect();
        for j in (primeira.obs + 1)..ultima.obs {
            if presentes.contains(&j) {
                continue;
            }
            let Some(antes) = c.aparicoes.iter().rev().find(|a| a.obs < j) else {
                continue;
            };
            if matches!(antes.cand.versao.frequencia, Frequencia::Unica { .. }) {
                continue;
            }
            let deslocamento = observadas[antes.obs]
                .competencia
                .meses_ate(antes.cand.versao.desde)
                .max(0);
            let competencia = observadas[j].competencia.somar_meses(deslocamento);
            if ativa(&excecoes_banco, competencia, |e| {
                matches!(e, Excecao::Dispensar { .. })
            }) {
                continue;
            }
            let aba = &plano.base[observadas[j].entrada].aba;
            passo(
                j,
                Acao::Dispensar {
                    competencia,
                    motivo: format!("não houve: não aparece na aba {aba}"),
                },
            );
        }

        // O pago de cada mês.
        for a in &c.aparicoes {
            let Some(p) = &a.cand.pago else { continue };
            if ativa(&excecoes_banco, p.competencia, |e| {
                matches!(e, Excecao::PagoInformado { .. })
            }) {
                continue;
            }
            passo(
                a.obs,
                Acao::Pago {
                    pago: p.clone(),
                    prazo,
                },
            );
        }

        // Fim: não está na aba mais recente lida.
        let e_unica = matches!(ultima.cand.versao.frequencia, Frequencia::Unica { .. });
        let ultimo_mes = ultima
            .cand
            .versao
            .desde
            .max(observadas[ultima.obs].competencia);
        let desde_do_fim = ultimo_mes.seguinte();
        if ultima.obs < ultima_obs && !e_unica {
            let cancelamento = rec_banco.and_then(|r| r.cancelamento.as_ref());
            match cancelamento {
                Some(x) if x.desde == desde_do_fim => {}
                Some(x) if x.comprovado => {
                    plano.base[entrada_de(ultima.obs)].conflitos += 1;
                }
                _ => {
                    let primeira_ausente = &plano.base[observadas[ultima.obs + 1].entrada].aba;
                    passo(
                        ultima.obs,
                        Acao::Cancelar {
                            desde: desde_do_fim,
                            motivo: format!(
                                "não aparece mais na planilha desde {primeira_ausente}"
                            ),
                        },
                    );
                }
            }
        } else if rec_banco.is_some_and(|r| r.cancelamento.is_some()) {
            // Voltou a aparecer, mas o banco o tem como cancelado: não reabre sozinho.
            plano.base[entrada_de(ultima.obs)].conflitos += 1;
        }
    }

    plano.observadas = observadas;
    plano.passos = passos;
    plano.por_apelido = por_apelido;
    plano.do_banco = alvos
        .iter()
        .map(|a| match a {
            Alvo::Existente(i) => {
                Some((existentes[*i].id.clone(), existentes[*i].versao_do_registro))
            }
            _ => None,
        })
        .collect();
    Ok(plano)
}

impl Plano {
    /// A prévia: o que seria gravado, sem gravar.
    pub fn previa(&self) -> RelatorioDoHistorico {
        let mut abas = self.base.clone();
        let mut grupos = 0;
        let mut detalhes = Vec::new();
        for p in &self.passos {
            if matches!(p.acao, Acao::Grupo(_)) {
                grupos += 1;
            } else {
                p.contar(&mut abas[p.entrada]);
            }
            detalhes.push(p.descrever());
        }
        RelatorioDoHistorico {
            abas,
            grupos,
            confirmado: false,
            detalhes,
            ..RelatorioDoHistorico::default()
        }
    }

    /// Grava tudo por `Planejamento::aplicar` (autor `Importacao`) e passa a aba mais
    /// recente pelo importador normal. Uma falha isolada não interrompe as outras.
    pub fn aplicar(
        &self,
        plan: &mut dyn Planejamento,
        mapa: &Mapa,
    ) -> Resultado<RelatorioDoHistorico> {
        let mut r = RelatorioDoHistorico {
            abas: self.base.clone(),
            confirmado: true,
            ..RelatorioDoHistorico::default()
        };
        let mut ids: Vec<Option<String>> = self
            .do_banco
            .iter()
            .map(|x| x.as_ref().map(|(id, _)| id.clone()))
            .collect();
        let mut bases: Vec<i64> = self
            .do_banco
            .iter()
            .map(|x| x.as_ref().map_or(0, |(_, b)| *b))
            .collect();
        for p in &self.passos {
            r.detalhes.push(p.descrever());
            let resultado = match &p.acao {
                Acao::Grupo(g) => plan.aplicar(
                    Alteracao::CriarGrupo {
                        nome: g.nome.clone(),
                        posicao: g.posicao,
                    },
                    Autor::Importacao,
                ),
                Acao::Criar { novo, origem } => plan.aplicar(
                    Alteracao::CriarRecorrente {
                        recorrente: novo.clone(),
                        origem: Some(origem.clone()),
                    },
                    Autor::Importacao,
                ),
                outra => {
                    // Se a criação falhou, o resto do compromisso fica sem alvo e é pulado.
                    let Some(id) = ids[p.compromisso].clone() else {
                        continue;
                    };
                    let alteracao = match outra {
                        Acao::Editar { desde, mudanca } => Alteracao::Editar {
                            recorrente: id,
                            escopo: Escopo::DaquiEmDiante(*desde),
                            mudanca: mudanca.clone(),
                            versao_base: bases[p.compromisso],
                        },
                        Acao::Dispensar {
                            competencia,
                            motivo,
                        } => Alteracao::Excecao {
                            ocorrencia: IdOcorrencia {
                                recorrente: id,
                                competencia: *competencia,
                                parcela: None,
                            },
                            excecao: Excecao::Dispensar {
                                motivo: motivo.clone(),
                            },
                        },
                        Acao::Pago { pago, prazo } => importacao::excecao_de_pago(&id, prazo, pago),
                        Acao::Cancelar { desde, motivo } => Alteracao::Cancelar {
                            recorrente: id,
                            desde: *desde,
                            motivo: motivo.clone(),
                            comprovado: false,
                            versao_base: bases[p.compromisso],
                        },
                        Acao::Grupo(_) | Acao::Criar { .. } => continue,
                    };
                    plan.aplicar(alteracao, Autor::Importacao)
                }
            };
            match resultado {
                Ok(recibo) => match &p.acao {
                    Acao::Grupo(_) => r.grupos += 1,
                    Acao::Criar { .. } => {
                        ids[p.compromisso] = Some(recibo.alvo);
                        bases[p.compromisso] = recibo.versao_nova;
                        p.contar(&mut r.abas[p.entrada]);
                    }
                    Acao::Editar { .. } | Acao::Cancelar { .. } => {
                        bases[p.compromisso] = recibo.versao_nova;
                        p.contar(&mut r.abas[p.entrada]);
                    }
                    Acao::Dispensar { .. } | Acao::Pago { .. } => p.contar(&mut r.abas[p.entrada]),
                },
                Err(e) => registrar_falha(&mut r, p.entrada, rotulo(&e)),
            }
        }

        // A aba mais recente pelo importador normal: composição do salário, adiados e o
        // que o mapa pedir. O pago já foi gravado acima.
        if let Some(o) = self.observadas.last() {
            let feito = importacao::previa_com_mapa(&o.aba, &self.sha, &*plan, o.competencia, mapa)
                .and_then(|mut p| {
                    for e in &mut p.existentes {
                        e.pago = None;
                    }
                    for a in &mut p.atualizaveis {
                        a.pago = None;
                    }
                    p.novos
                        .retain(|n| !self.por_apelido.contains(&normalizar(&n.recorrente.nome)));
                    importacao::confirmar(&p, plan)
                });
            match feito {
                Ok(c) => {
                    let a = &mut r.abas[o.entrada];
                    a.novos += c.criados;
                    a.versoes += c.atualizados;
                    a.pagos += c.pagos_informados;
                    r.grupos += c.grupos;
                    r.composicoes += c.composicoes;
                    r.adiadas += c.adiadas;
                }
                Err(_) => registrar_falha(
                    &mut r,
                    o.entrada,
                    "composição e ajustes da aba mais recente",
                ),
            }
        }
        Ok(r)
    }
}

fn registrar_falha(r: &mut RelatorioDoHistorico, entrada: usize, motivo: &str) {
    r.abas[entrada].falhas += 1;
    let texto = format!("{}: {motivo}", r.abas[entrada].aba);
    r.falhas.push(texto);
}

/// Lê as abas mensais `AAAA.MM` do arquivo (as outras são ignoradas), em ordem
/// cronológica, e importa o histórico. Sem `confirmar`, só a prévia.
pub fn importar_historico(
    arquivo: &Path,
    plan: &mut dyn Planejamento,
    mapa: &Mapa,
    janela: Janela,
    confirmar: bool,
) -> Resultado<RelatorioDoHistorico> {
    let sha = leitura::sha256_do_arquivo(arquivo)?;
    let mut mensais: Vec<(Competencia, String)> = leitura::nomes_das_abas(arquivo)?
        .into_iter()
        .filter_map(|n| leitura::competencia_da_aba(&n).map(|c| (c, n)))
        .filter(|(c, _)| janela.contem(*c))
        .collect();
    mensais.sort();
    mensais.dedup_by_key(|(c, _)| *c);
    let nomes: Vec<String> = mensais.iter().map(|(_, n)| n.clone()).collect();
    let lidas = leitura::ler_abas(arquivo, &nomes)?;
    let entradas: Vec<EntradaDeAba> = mensais
        .into_iter()
        .zip(lidas)
        .map(|((competencia, nome), (_, aba))| EntradaDeAba {
            nome,
            competencia,
            // O motivo é genérico: a mensagem do leitor pode trazer caminho.
            aba: aba.map_err(|_| "ilegível".to_owned()),
        })
        .collect();
    let plano = planejar(entradas, &sha, &*plan, mapa)?;
    if confirmar {
        plano.aplicar(plan, mapa)
    } else {
        Ok(plano.previa())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fixtura::{AbaDoMes, aba_do_mes, linha, planilha_historica};
    use crate::importacao;
    use crate::leitura;
    use sbf_planejamento::{
        Alteracao, Banco, Data, EstadoDaOcorrencia, Excecao, Filtro, Ocorrencia, Periodo,
        PlanejamentoSqlite, SemSaldos,
    };

    fn banco() -> PlanejamentoSqlite {
        PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
    }

    fn hoje() -> Data {
        Data::nova(2026, 11, 18).expect("data")
    }

    fn comp(ano: i32, mes: u8) -> Competencia {
        Competencia::nova(ano, mes).expect("competencia")
    }

    fn arquivo(abas: &[AbaDoMes]) -> (tempfile::TempDir, std::path::PathBuf) {
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("hist.xlsx");
        planilha_historica(&arq, abas);
        (pasta, arq)
    }

    fn rodar(arq: &Path, plan: &mut PlanejamentoSqlite, confirmar: bool) -> RelatorioDoHistorico {
        importar_historico(arq, plan, &Mapa::vazio(), Janela::default(), confirmar)
            .expect("historico")
    }

    fn por_nome(plan: &PlanejamentoSqlite, nome: &str) -> sbf_planejamento::Recorrente {
        let mut achados: Vec<_> = plan
            .recorrentes(false)
            .expect("recorrentes")
            .into_iter()
            .filter(|r| r.nome == nome)
            .collect();
        assert_eq!(achados.len(), 1, "esperava um só '{nome}'");
        achados.remove(0)
    }

    /// Quatro meses: A termina em 2026.02; B muda de valor em 2026.03; C falta em 2026.02;
    /// D só aparece no fim.
    fn quatro_meses() -> Vec<AbaDoMes> {
        vec![
            aba_do_mes(
                "2026.01",
                vec![
                    linha("Casa", "Item A", 5, 100.0, Some(100.0)),
                    linha("", "Item B", 6, 50.0, Some(50.0)),
                    linha("", "Item C", 7, 70.0, None),
                ],
            ),
            aba_do_mes(
                "2026.02",
                vec![
                    linha("Casa", "Item A", 5, 100.0, None),
                    linha("", "Item B", 6, 50.0, None),
                ],
            ),
            aba_do_mes(
                "2026.03",
                vec![
                    linha("Casa", "Item B", 6, 60.0, None),
                    linha("", "Item C", 7, 70.0, None),
                ],
            ),
            aba_do_mes(
                "2026.04",
                vec![
                    linha("Casa", "Item B", 6, 60.0, None),
                    linha("", "Item C", 7, 70.0, None),
                    linha("", "Item D", 8, 10.0, None),
                ],
            ),
        ]
    }

    #[test]
    fn vigencia_vai_do_primeiro_ao_ultimo_mes_e_o_que_sumiu_e_encerrado() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        let r = rodar(&arq, &mut plan, true);
        assert!(r.confirmado);
        assert_eq!(plan.recorrentes(false).expect("r").len(), 4);

        let a = por_nome(&plan, "Item A");
        assert_eq!(a.versoes[0].desde, comp(2026, 1));
        let c = a.cancelamento.expect("A foi encerrado");
        assert_eq!(c.desde, comp(2026, 3));
        assert!(c.motivo.contains("2026.03"), "{}", c.motivo);
        assert!(!c.comprovado);

        // B não termina: continua na aba mais recente.
        let b = por_nome(&plan, "Item B");
        assert!(b.cancelamento.is_none());
        // Valor mudou em 2026.03: duas versões.
        assert_eq!(b.versoes.len(), 2);
        assert_eq!(b.versoes[1].desde, comp(2026, 3));

        // D nasce em 2026.04.
        assert_eq!(por_nome(&plan, "Item D").versoes[0].desde, comp(2026, 4));

        let t = r.total();
        assert_eq!(t.novos, 4);
        assert_eq!(t.encerrados, 1);
        assert_eq!(t.versoes, 1);
        assert_eq!(t.falhas, 0);
    }

    #[test]
    fn mes_do_meio_sem_o_compromisso_vira_excecao_nao_houve() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let c = por_nome(&plan, "Item C");
        let excecoes = plan.excecoes_do_recorrente(&c.id).expect("excecoes");
        let dispensas: Vec<_> = excecoes
            .iter()
            .filter(|e| matches!(e.excecao, Excecao::Dispensar { .. }))
            .collect();
        assert_eq!(dispensas.len(), 1);
        assert_eq!(dispensas[0].ocorrencia.competencia, comp(2026, 2));
        if let Excecao::Dispensar { motivo } = &dispensas[0].excecao {
            assert!(motivo.contains("não houve"), "{motivo}");
        }
        let o = plan
            .proximas(&c.id, comp(2026, 2), 1, hoje())
            .expect("proximas");
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Dispensada);
    }

    #[test]
    fn pago_de_cada_mes_vira_pago_informado() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        let r = rodar(&arq, &mut plan, true);
        // A (100,00) e B (50,00) pagos em 2026.01.
        assert_eq!(r.total().pagos, 2);
        let a = por_nome(&plan, "Item A");
        let o = plan
            .proximas(&a.id, comp(2026, 1), 1, hoje())
            .expect("proximas");
        assert_eq!(o[0].estado, EstadoDaOcorrencia::PagoInformado);
        assert_eq!(
            o[0].pago_informado,
            Some(sbf_planejamento::Centavos(10_000))
        );
    }

    #[test]
    fn previa_nao_grava_nada_e_conta_o_que_faria() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        let r = rodar(&arq, &mut plan, false);
        assert!(!r.confirmado);
        assert!(plan.recorrentes(true).expect("r").is_empty());
        assert!(plan.grupos().expect("g").is_empty());
        let t = r.total();
        assert_eq!((t.novos, t.encerrados, t.excecoes, t.versoes), (4, 1, 1, 1));
    }

    #[test]
    fn layout_antigo_sem_grupo_herda_do_mes_posterior_ou_vai_para_planilha_antiga() {
        let mut abas = vec![
            AbaDoMes {
                com_grupo: false,
                ..aba_do_mes(
                    "2025.11",
                    vec![
                        linha("", "Item B", 6, 50.0, None),
                        linha("", "Item Morto", 9, 5.0, None),
                    ],
                )
            },
            AbaDoMes {
                com_grupo: false,
                ..aba_do_mes("2025.12", vec![linha("", "Item B", 6, 50.0, None)])
            },
        ];
        abas.extend(quatro_meses());
        let (_p, arq) = arquivo(&abas);
        let mut plan = banco();
        let r = rodar(&arq, &mut plan, true);
        assert_eq!(r.total().falhas, 0, "{:?}", r.falhas);
        assert_eq!(por_nome(&plan, "Item B").grupo, "Casa");
        assert_eq!(por_nome(&plan, "Item Morto").grupo, "Planilha antiga");
        let grupos: Vec<String> = plan
            .grupos()
            .expect("g")
            .into_iter()
            .map(|g| g.nome)
            .collect();
        assert!(grupos.contains(&"Planilha antiga".to_owned()));
        // B começa em 2025.11, o primeiro mês em que aparece.
        assert_eq!(por_nome(&plan, "Item B").versoes[0].desde, comp(2025, 11));
    }

    #[test]
    fn aba_sem_linhas_vira_nao_lida_e_nao_cria_buraco() {
        let abas = vec![
            aba_do_mes("2026.01", vec![]),
            aba_do_mes("2026.02", vec![linha("Casa", "Item B", 6, 50.0, None)]),
            aba_do_mes("2026.03", vec![]),
            aba_do_mes("2026.04", vec![linha("Casa", "Item B", 6, 50.0, None)]),
        ];
        let (_p, arq) = arquivo(&abas);
        let mut plan = banco();
        let r = rodar(&arq, &mut plan, true);
        assert_eq!(r.abas.len(), 4);
        assert_eq!(r.abas[0].nao_lida.as_deref(), Some("layout"));
        assert_eq!(r.abas[2].nao_lida.as_deref(), Some("layout"));
        assert!(r.abas[1].nao_lida.is_none());
        // 2026.03 não foi lida: B não ganha dispensa nem fim por causa dela.
        let b = por_nome(&plan, "Item B");
        assert!(b.cancelamento.is_none());
        assert!(plan.excecoes_do_recorrente(&b.id).expect("e").is_empty());
        assert_eq!(b.versoes[0].desde, comp(2026, 2));
    }

    #[test]
    fn idempotente_rodar_duas_vezes_nao_duplica_nada() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let antes = (
            plan.recorrentes(true).expect("r").len(),
            plan.historico(None, 10_000).expect("h").len(),
        );
        let segunda = rodar(&arq, &mut plan, true);
        let depois = (
            plan.recorrentes(true).expect("r").len(),
            plan.historico(None, 10_000).expect("h").len(),
        );
        assert_eq!(antes, depois);
        let t = segunda.total();
        assert_eq!(
            (
                t.novos,
                t.encerrados,
                t.excecoes,
                t.pagos,
                t.versoes,
                t.falhas
            ),
            (0, 0, 0, 0, 0, 0)
        );
    }

    fn linhas_do_controle(
        plan: &PlanejamentoSqlite,
        de: Competencia,
        meses: i32,
    ) -> Vec<(String, Ocorrencia)> {
        let mut saida = Vec::new();
        for i in 0..meses {
            let c = de.somar_meses(i);
            let controle = plan
                .controle(&Periodo::MesCivil(c), Filtro::Todos, None, hoje())
                .expect("controle");
            for o in controle.linhas {
                saida.push((c.to_string(), o));
            }
        }
        saida
    }

    fn resumo(l: &(String, Ocorrencia)) -> String {
        let o = &l.1;
        format!(
            "{} {} {} {:?} {} {:?} {:?} {:?} {:?}",
            l.0,
            o.grupo,
            o.nome,
            o.previsto,
            o.vencimento,
            o.estado,
            o.certeza,
            o.conta,
            o.natureza
        )
    }

    #[test]
    fn invariante_projecao_da_aba_mais_recente_em_diante_e_a_de_importar_so_ela() {
        let mut abas = quatro_meses();
        abas[3].salario = Some(5000.0);
        abas[2].salario = Some(4800.0);
        let (_p, arq) = arquivo(&abas);

        let mut com_historico = banco();
        rodar(&arq, &mut com_historico, true);

        let mut so_a_ultima = banco();
        let sha = leitura::sha256_do_arquivo(&arq).expect("sha");
        let aba = leitura::ler_aba(&arq, "2026.04").expect("aba");
        let p = importacao::previa(&aba, &sha, &so_a_ultima, comp(2026, 4)).expect("previa");
        importacao::confirmar(&p, &mut so_a_ultima).expect("confirmar");

        let a = linhas_do_controle(&com_historico, comp(2026, 4), 14);
        let b = linhas_do_controle(&so_a_ultima, comp(2026, 4), 14);
        let mut ra: Vec<String> = a.iter().map(resumo).collect();
        let mut rb: Vec<String> = b.iter().map(resumo).collect();
        ra.sort();
        rb.sort();
        assert!(!ra.is_empty());
        assert_eq!(ra, rb);
        // A ordem dos grupos também é a mesma.
        let ga: Vec<_> = com_historico.grupos().expect("g");
        let gb: Vec<_> = so_a_ultima.grupos().expect("g");
        assert_eq!(&ga[..gb.len()], &gb[..]);
        // O encerrado não aparece depois do último mês visto.
        assert!(a.iter().all(|(_, o)| o.nome != "Item A"));
    }

    #[test]
    fn encerrado_nao_aparece_depois_do_ultimo_mes_visto_nem_com_o_horizonte_longo() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let linhas = linhas_do_controle(&plan, comp(2026, 3), 30);
        assert!(linhas.iter().all(|(_, o)| o.nome != "Item A"));
        let antes = linhas_do_controle(&plan, comp(2026, 1), 2);
        assert!(antes.iter().any(|(_, o)| o.nome == "Item A"));
    }

    #[test]
    fn estende_para_tras_o_recorrente_que_ja_existia_sem_duplicar() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        // Já importou só a aba mais recente, como antes.
        let sha = leitura::sha256_do_arquivo(&arq).expect("sha");
        let aba = leitura::ler_aba(&arq, "2026.04").expect("aba");
        let p = importacao::previa(&aba, "outro-sha", &plan, comp(2026, 4)).expect("previa");
        importacao::confirmar(&p, &mut plan).expect("confirmar");
        assert_eq!(plan.recorrentes(false).expect("r").len(), 3);
        let _ = sha;

        let r = rodar(&arq, &mut plan, true);
        // A (encerrado) é o único criado; B, C e D foram casados por nome.
        assert_eq!(plan.recorrentes(false).expect("r").len(), 4);
        assert_eq!(r.total().novos, 1);
        let b = por_nome(&plan, "Item B");
        assert_eq!(b.versoes[0].desde, comp(2026, 1));
        // Janeiro (50), março (60) e a de abril que já existia (60): o histórico só
        // acrescenta, nunca mexe na versão que já estava lá.
        assert_eq!(b.versoes.len(), 3);
        // De novo: nada muda.
        let r2 = rodar(&arq, &mut plan, true);
        let t = r2.total();
        assert_eq!((t.novos, t.versoes, t.excecoes, t.pagos), (0, 0, 0, 0));
    }

    #[test]
    fn janela_de_ate_limita_as_abas() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        let r = importar_historico(
            &arq,
            &mut plan,
            &Mapa::vazio(),
            Janela {
                de: Some(comp(2026, 2)),
                ate: Some(comp(2026, 3)),
            },
            true,
        )
        .expect("historico");
        assert_eq!(r.abas.len(), 2);
        assert_eq!(por_nome(&plan, "Item A").versoes[0].desde, comp(2026, 2));
        assert!(
            plan.recorrentes(false)
                .expect("r")
                .iter()
                .all(|r| r.nome != "Item D")
        );
    }

    #[test]
    fn valor_com_erro_de_formula_no_meio_nao_apaga_o_valor_anterior() {
        // Previsto ausente em 2026.02 (como erro de fórmula): carrega o de 2026.01.
        let mut abas = quatro_meses();
        abas[1].linhas[1].previsto = None;
        let (_p, arq) = arquivo(&abas);
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let b = por_nome(&plan, "Item B");
        assert_eq!(b.versoes.len(), 2);
        assert_eq!(b.versoes[0].valor, Some(sbf_planejamento::Centavos(5_000)));
    }

    #[test]
    fn arquivo_que_cresce_um_mes_so_acrescenta() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);

        // Chega maio: D some e B muda de valor. É outro arquivo (outro sha).
        let mut abas = quatro_meses();
        abas.push(aba_do_mes(
            "2026.05",
            vec![
                linha("Casa", "Item B", 6, 65.0, None),
                linha("", "Item C", 7, 70.0, None),
            ],
        ));
        let (_p2, arq2) = arquivo(&abas);
        let r = rodar(&arq2, &mut plan, true);
        let t = r.total();
        assert_eq!((t.novos, t.encerrados, t.versoes, t.falhas), (0, 1, 1, 0));
        assert_eq!(plan.recorrentes(false).expect("r").len(), 4);
        assert_eq!(
            por_nome(&plan, "Item D").cancelamento.expect("fim").desde,
            comp(2026, 5)
        );
        assert_eq!(por_nome(&plan, "Item B").versoes.len(), 3);
        // A continua encerrado onde estava.
        assert_eq!(
            por_nome(&plan, "Item A").cancelamento.expect("fim").desde,
            comp(2026, 3)
        );
        let r2 = rodar(&arq2, &mut plan, true);
        let t = r2.total();
        assert_eq!(
            (t.novos, t.encerrados, t.versoes, t.excecoes, t.pagos),
            (0, 0, 0, 0, 0)
        );
    }

    #[test]
    fn valor_decidido_pelo_usuario_nao_e_sobrescrito() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let b = por_nome(&plan, "Item B");
        // O usuário decide o valor de B de março em diante.
        plan.aplicar(
            Alteracao::Editar {
                recorrente: b.id.clone(),
                escopo: sbf_planejamento::Escopo::DaquiEmDiante(comp(2026, 3)),
                mudanca: sbf_planejamento::MudancaDeVersao {
                    valor: Some(Some(sbf_planejamento::Centavos(9_900))),
                    fonte: Some(sbf_planejamento::Fonte::Usuario),
                    ..Default::default()
                },
                versao_base: b.versao_do_registro,
            },
            sbf_planejamento::Autor::Usuario,
        )
        .expect("editar");
        // Outro arquivo, em que B mudaria em abril.
        let mut abas = quatro_meses();
        abas[3].linhas[0].previsto = Some(70.0);
        let (_p2, arq2) = arquivo(&abas);
        let r = rodar(&arq2, &mut plan, true);
        assert!(r.total().conflitos >= 1);
        let b = por_nome(&plan, "Item B");
        assert!(
            b.versoes
                .iter()
                .all(|v| v.valor != Some(sbf_planejamento::Centavos(7_000)))
        );
    }

    fn criar_do_usuario(plan: &mut PlanejamentoSqlite, nome: &str, grupo: &str) -> String {
        use sbf_planejamento::{Certeza, Frequencia, Natureza, NovoRecorrente, Prazo, Versao};
        plan.aplicar(
            Alteracao::CriarGrupo {
                nome: grupo.to_owned(),
                posicao: 50,
            },
            sbf_planejamento::Autor::Usuario,
        )
        .expect("grupo");
        plan.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: nome.to_owned(),
                    grupo: grupo.to_owned(),
                    subgrupo: None,
                    posicao: 1,
                    natureza: Natureza::Saida,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: comp(2026, 3),
                        valor: Some(sbf_planejamento::Centavos(1_000)),
                        certeza: Certeza::Estimado,
                        fonte: sbf_planejamento::Fonte::Pluggy,
                        frequencia: Frequencia::Mensal { dia: 3 },
                        conta: None,
                        centro: None,
                        fornecedor: None,
                        categoria: None,
                        nota: None,
                    },
                },
                origem: None,
            },
            sbf_planejamento::Autor::Usuario,
        )
        .expect("criar")
        .alvo
    }

    #[test]
    fn renomeado_no_app_continua_o_mesmo_compromisso() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        rodar(&arq, &mut plan, true);
        let c = por_nome(&plan, "Item C");
        plan.aplicar(
            Alteracao::Renomear {
                recorrente: c.id.clone(),
                nome: "Item C curto".to_owned(),
                versao_base: c.versao_do_registro,
            },
            sbf_planejamento::Autor::Usuario,
        )
        .expect("renomear");
        let antes = plan.recorrentes(true).expect("r").len();

        // O mesmo arquivo (casa pela origem) e um arquivo maior (outro sha: casa pelo
        // nome com que nasceu).
        let r = rodar(&arq, &mut plan, true);
        assert_eq!(r.total().novos, 0);
        let mut abas = quatro_meses();
        abas.push(aba_do_mes(
            "2026.05",
            vec![
                linha("Casa", "Item B", 6, 60.0, None),
                linha("", "Item C", 7, 70.0, None),
            ],
        ));
        let (_p2, arq2) = arquivo(&abas);
        let r = rodar(&arq2, &mut plan, true);
        assert_eq!(r.total().novos, 0, "{:?}", r.falhas);
        assert_eq!(plan.recorrentes(true).expect("r").len(), antes);
        assert!(
            plan.recorrentes(false)
                .expect("r")
                .iter()
                .all(|r| r.nome != "Item C")
        );
        // E a prévia também não o criaria.
        let r = rodar(&arq2, &mut plan, false);
        assert_eq!(r.total().novos, 0);
    }

    #[test]
    fn so_o_que_nasceu_da_planilha_e_encerrado_ou_dispensado() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        // Um achado da Pluggy que a planilha não tem, e outro com o mesmo nome de um item
        // que a planilha encerrou (Item A).
        let achado = criar_do_usuario(&mut plan, "Achado Pluggy", "Achados na Pluggy");
        let homonimo = criar_do_usuario(&mut plan, "Item A", "Achados na Pluggy");
        let r = rodar(&arq, &mut plan, true);
        assert_eq!(r.total().falhas, 0, "{:?}", r.falhas);
        for id in [&achado, &homonimo] {
            let rec = plan.recorrente(id).expect("r").expect("existe");
            assert!(rec.cancelamento.is_none());
            assert_eq!(rec.versoes.len(), 1);
            assert!(plan.excecoes_do_recorrente(id).expect("e").is_empty());
        }
        // O Item A da planilha nasceu à parte e foi encerrado.
        let nascidos: Vec<_> = plan
            .recorrentes(false)
            .expect("r")
            .into_iter()
            .filter(|r| r.nome == "Item A" && r.id != homonimo)
            .collect();
        assert_eq!(nascidos.len(), 1);
        assert!(nascidos[0].cancelamento.is_some());
    }

    #[test]
    fn nao_planilha_na_aba_mais_recente_casa_so_pelo_mes_mais_recente() {
        let (_p, arq) = arquivo(&quatro_meses());
        let mut plan = banco();
        // Mesmo nome de um item da última aba (Item D), mas criado por fora.
        let d = criar_do_usuario(&mut plan, "Item D", "Achados na Pluggy");
        let r = rodar(&arq, &mut plan, true);
        assert_eq!(r.total().falhas, 0, "{:?}", r.falhas);
        let ds: Vec<_> = plan
            .recorrentes(false)
            .expect("r")
            .into_iter()
            .filter(|r| r.nome == "Item D")
            .collect();
        assert_eq!(ds.len(), 1, "não duplica");
        assert_eq!(ds[0].id, d);
        assert!(ds[0].cancelamento.is_none());
    }
}
