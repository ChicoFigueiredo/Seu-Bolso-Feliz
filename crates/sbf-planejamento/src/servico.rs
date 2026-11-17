//! [`PlanejamentoSqlite`]: o serviço único do controle mensal sobre o SQLite.
//!
//! Leitura: tudo é projetado das regras na hora; nada é materializado. Escrita: toda
//! alteração passa por `escrita::aplicar_na`, numa transação, com a linha de histórico.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use rusqlite::{Connection, OptionalExtension, params};

mod extrato;

use crate::banco::{
    self, Banco, Dados, agora_unix, carregar_dados, carregar_um, de_json, de_rfc3339, json,
    novo_id, rfc3339,
};
use crate::conciliacao::normalizar;
use crate::escrita::aplicar_na;
use crate::projecao::{projetar_competencias_desde, projetar_desde};
use crate::resumo;
use crate::{
    Alteracao, AlvoDoDocumento, Assinatura, Autor, Centavos, Competencia, Composicao, Conciliacao,
    ContaDoPlanejamento, Controle, Data, DocumentoVinculado, EntradaDoHistorico,
    ErroDoPlanejamento, EstadoDaOcorrencia, EstadoDaProposta, Excecao, ExcecaoGravada, Filtro,
    FonteDeMovimentos, FonteDeSaldos, Grupo, HORIZONTE_MESES, MovimentoCandidato, Natureza,
    Ocorrencia, OrigemImportada, ParDeConta, Periodo, Planejamento, Proposta, Recibo, Recorrente,
    Resultado, SemMovimentos, Simulacao, TotaisDoMes,
};

/// Quantos meses para trás o controle procura o que está em atraso.
const MESES_DE_ATRASO: i32 = 24;
/// A janela da simulação, antes e depois.
const MESES_DA_SIMULACAO: u16 = 24;
/// Quanto tempo uma proposta espera confirmação: 7 dias.
const VALIDADE_DA_PROPOSTA_S: i64 = 7 * 24 * 3600;

pub struct PlanejamentoSqlite {
    pub banco: Banco,
    pub saldos: Box<dyn FonteDeSaldos>,
    /// O extrato (a Pluggy no inventário); sem ele, nenhuma sugestão de conciliação.
    pub movimentos: Box<dyn FonteDeMovimentos>,
    relogio: Box<dyn Fn() -> i64 + Send>,
}

impl fmt::Debug for PlanejamentoSqlite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlanejamentoSqlite")
            .field("banco", &self.banco)
            .finish_non_exhaustive()
    }
}

impl PlanejamentoSqlite {
    pub fn novo(banco: Banco, saldos: Box<dyn FonteDeSaldos>) -> PlanejamentoSqlite {
        PlanejamentoSqlite {
            banco,
            saldos,
            movimentos: Box::new(SemMovimentos),
            relogio: Box::new(agora_unix),
        }
    }

    /// Liga o extrato: de onde vêm os candidatos, as sugestões de conciliação e as contas.
    #[must_use]
    pub fn com_movimentos(mut self, movimentos: Box<dyn FonteDeMovimentos>) -> PlanejamentoSqlite {
        self.movimentos = movimentos;
        self
    }

    /// Troca o relógio (segundos Unix UTC): os testes fixam o "agora" das propostas e do
    /// histórico.
    #[must_use]
    pub fn com_relogio(mut self, relogio: impl Fn() -> i64 + Send + 'static) -> PlanejamentoSqlite {
        self.relogio = Box::new(relogio);
        self
    }

    fn agora(&self) -> i64 {
        (self.relogio)()
    }
}

// ---------------------------------------------------------------------------------------
// Leitura: ajudantes
// ---------------------------------------------------------------------------------------

fn passa_no_filtro(o: &Ocorrencia, filtro: Filtro, hoje: Data) -> bool {
    use EstadoDaOcorrencia::*;
    if filtro == Filtro::PausadosECancelados {
        return o.suprimida();
    }
    if o.suprimida() {
        return false;
    }
    match filtro {
        Filtro::Todos => true,
        Filtro::APagar => matches!(o.estado, Prevista | Adiada | Parcial | EmAtraso),
        Filtro::EmAtraso => o.devida_vencida(hoje),
        Filtro::Pagos => matches!(o.estado, PagoInformado | Conciliada),
        Filtro::SemComprovante => o.pago_informado.is_some() && o.conciliado.is_none(),
        Filtro::SemValor => o.previsto.is_none(),
        Filtro::PausadosECancelados => false,
    }
}

/// As competências a projetar: o intervalo e as das exceções de adiar (que podem mudar o
/// mês do vencimento).
fn candidatas(
    de: Competencia,
    ate: Competencia,
    excecoes: &[ExcecaoGravada],
) -> BTreeSet<Competencia> {
    let mut conjunto: BTreeSet<Competencia> =
        (0..=de.meses_ate(ate)).map(|i| de.somar_meses(i)).collect();
    conjunto.extend(
        excecoes
            .iter()
            .filter(|e| !e.desfeita && matches!(e.excecao, Excecao::Adiar { .. }))
            .map(|e| e.ocorrencia.competencia),
    );
    conjunto
}

fn ativos(dados: &Dados) -> impl Iterator<Item = &Recorrente> {
    dados
        .recorrentes
        .iter()
        .filter(|r| !r.arquivado && r.excluido.is_none())
}

fn posicoes_dos_grupos(grupos: &[Grupo]) -> HashMap<&str, i64> {
    grupos
        .iter()
        .map(|g| (g.nome.as_str(), g.posicao))
        .collect()
}

fn ordenar(linhas: &mut [Ocorrencia], grupos: &[Grupo]) {
    let posicoes = posicoes_dos_grupos(grupos);
    linhas.sort_by(|a, b| {
        let ga = posicoes.get(a.grupo.as_str()).copied().unwrap_or(i64::MAX);
        let gb = posicoes.get(b.grupo.as_str()).copied().unwrap_or(i64::MAX);
        ga.cmp(&gb)
            .then_with(|| a.grupo.cmp(&b.grupo))
            .then_with(|| a.posicao.cmp(&b.posicao))
            .then_with(|| a.vencimento.cmp(&b.vencimento))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// O que a simulação compara por mês: nada, ou (previsto, vencimento, estado).
type Vista = Option<(Option<Centavos>, Data, EstadoDaOcorrencia)>;

fn vista(o: Option<&Ocorrencia>) -> Vista {
    o.filter(|x| !x.suprimida())
        .map(|x| (x.previsto, x.vencimento, x.estado))
}

struct Foto {
    ocorrencias: Vec<Ocorrencia>,
    excecoes: Vec<ExcecaoGravada>,
}

fn fotografar(
    c: &Connection,
    recorrente: Option<&str>,
    de: Competencia,
    meses: u16,
    hoje: Data,
) -> Resultado<Foto> {
    let Some(id) = recorrente else {
        return Ok(Foto {
            ocorrencias: Vec::new(),
            excecoes: Vec::new(),
        });
    };
    let Some((rec, excecoes, conciliacoes)) = carregar_um(c, id)? else {
        return Ok(Foto {
            ocorrencias: Vec::new(),
            excecoes: Vec::new(),
        });
    };
    if meses == 0 {
        return Ok(Foto {
            ocorrencias: Vec::new(),
            excecoes,
        });
    }
    let ocorrencias = if rec.excluido.is_some() || rec.arquivado {
        Vec::new()
    } else {
        projetar_desde(
            &rec,
            &excecoes,
            &conciliacoes,
            de,
            de.somar_meses(i32::from(meses) - 1),
            hoje,
            banco::ler_inicio(c)?,
        )
    };
    Ok(Foto {
        ocorrencias,
        excecoes,
    })
}

fn recorrente_do_alvo(c: &Connection, alteracao: &Alteracao) -> Resultado<Option<String>> {
    Ok(match alteracao {
        Alteracao::Editar { recorrente, .. }
        | Alteracao::Pausar { recorrente, .. }
        | Alteracao::Retomar { recorrente, .. }
        | Alteracao::Cancelar { recorrente, .. }
        | Alteracao::Excluir { recorrente, .. }
        | Alteracao::Restaurar { recorrente, .. }
        | Alteracao::Arquivar { recorrente, .. }
        | Alteracao::Mover { recorrente, .. } => Some(recorrente.clone()),
        Alteracao::Excecao { ocorrencia, .. } => Some(ocorrencia.recorrente.clone()),
        Alteracao::Conciliar { conciliacao } => Some(conciliacao.ocorrencia.recorrente.clone()),
        Alteracao::Desconciliar { ocorrencia, .. } => Some(ocorrencia.recorrente.clone()),
        Alteracao::DesfazerExcecao { id } => c
            .query_row(
                "SELECT recorrente_id FROM excecoes WHERE id = ?1",
                params![id],
                |l| l.get(0),
            )
            .optional()?,
        Alteracao::DefinirComposicao { composicao, .. } => Some(composicao.recorrente.clone()),
        Alteracao::VincularDocumento { alvo, .. } => Some(match alvo {
            crate::AlvoDoDocumento::Recorrente { id } => id.clone(),
            crate::AlvoDoDocumento::Ocorrencia { id } => id.recorrente.clone(),
        }),
        Alteracao::CriarRecorrente { .. }
        | Alteracao::CriarGrupo { .. }
        | Alteracao::DesvincularDocumento { .. }
        | Alteracao::MapearConta { .. } => None,
        Alteracao::DefinirAssinatura { recorrente, .. } => Some(recorrente.clone()),
    })
}

/// De onde a simulação olha: a competência que a alteração toca, ou o mês de hoje.
fn inicio_da_simulacao(alteracao: &Alteracao, hoje: Data) -> Competencia {
    use crate::Escopo;
    match alteracao {
        Alteracao::Editar {
            escopo: Escopo::SoEsteMes(c) | Escopo::DaquiEmDiante(c),
            ..
        } => *c,
        Alteracao::Pausar { desde, .. }
        | Alteracao::Retomar { desde, .. }
        | Alteracao::Cancelar { desde, .. } => *desde,
        Alteracao::Excecao { ocorrencia, .. } => ocorrencia.competencia,
        Alteracao::Conciliar { conciliacao } => conciliacao.ocorrencia.competencia,
        Alteracao::CriarRecorrente { recorrente, .. } => recorrente.primeira_versao.desde,
        _ => Competencia::de_data(hoje),
    }
}

/// O que a alteração atinge e fica como está: pago informado, conciliação e valor só de um
/// mês. Sem valores nem nomes.
fn conflitos_da(alteracao: &Alteracao, inicio: Competencia, depois: &Foto) -> Vec<String> {
    use crate::Escopo;
    let mut conflitos = Vec::new();
    match alteracao {
        Alteracao::Pausar { .. } | Alteracao::Cancelar { .. } => {
            for o in depois.ocorrencias.iter().filter(|o| {
                o.id.competencia >= inicio && (o.pago_informado.is_some() || o.conciliado.is_some())
            }) {
                conflitos.push(format!(
                    "Em {} há pagamento informado ou conciliado: a ocorrência continua visível e devida.",
                    o.id.competencia
                ));
            }
        }
        Alteracao::Editar {
            escopo: Escopo::DaquiEmDiante(_),
            ..
        } => {
            for e in depois.excecoes.iter().filter(|e| {
                !e.desfeita
                    && e.ocorrencia.competencia >= inicio
                    && matches!(e.excecao, Excecao::ValorSoDesteMes { .. })
            }) {
                conflitos.push(format!(
                    "Em {} há um valor só deste mês: a versão nova não o altera.",
                    e.ocorrencia.competencia
                ));
            }
            for o in depois.ocorrencias.iter().filter(|o| {
                o.id.competencia >= inicio && (o.pago_informado.is_some() || o.conciliado.is_some())
            }) {
                conflitos.push(format!(
                    "Em {} há pagamento informado ou conciliado: ele fica como está.",
                    o.id.competencia
                ));
            }
        }
        Alteracao::DefinirComposicao { .. }
        | Alteracao::VincularDocumento { .. }
        | Alteracao::DesvincularDocumento { .. } => {
            conflitos.push("Não muda a projeção: só o detalhe e os documentos.".to_owned());
        }
        _ => {}
    }
    conflitos
}

// ---------------------------------------------------------------------------------------
// Propostas: ajudantes
// ---------------------------------------------------------------------------------------

fn texto_do_estado(e: EstadoDaProposta) -> &'static str {
    match e {
        EstadoDaProposta::Pendente => "pendente",
        EstadoDaProposta::Confirmada => "confirmada",
        EstadoDaProposta::Recusada => "recusada",
        EstadoDaProposta::Expirada => "expirada",
    }
}

fn estado_do_texto(t: &str) -> Resultado<EstadoDaProposta> {
    Ok(match t {
        "pendente" => EstadoDaProposta::Pendente,
        "confirmada" => EstadoDaProposta::Confirmada,
        "recusada" => EstadoDaProposta::Recusada,
        "expirada" => EstadoDaProposta::Expirada,
        _ => {
            return Err(ErroDoPlanejamento::Banco(
                "estado de proposta desconhecido".into(),
            ));
        }
    })
}

const COLUNAS_DE_PROPOSTA: &str =
    "id, alteracao, origem, chave, criada_em, expira_em, estado, motivo";

type LinhaDeProposta = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);

fn ler_linha_de_proposta(l: &rusqlite::Row<'_>) -> rusqlite::Result<LinhaDeProposta> {
    Ok((
        l.get(0)?,
        l.get(1)?,
        l.get(2)?,
        l.get(3)?,
        l.get(4)?,
        l.get(5)?,
        l.get(6)?,
        l.get(7)?,
    ))
}

/// Uma proposta pendente além da validade aparece como expirada, sem escrever nada.
fn montar_proposta(linha: LinhaDeProposta, agora: i64) -> Resultado<Proposta> {
    let (id, alteracao, origem, chave, criada_em, expira_em, estado, motivo) = linha;
    let mut estado = estado_do_texto(&estado)?;
    let mut motivo = motivo;
    if estado == EstadoDaProposta::Pendente && de_rfc3339(&expira_em).is_some_and(|t| t <= agora) {
        estado = EstadoDaProposta::Expirada;
        motivo.get_or_insert_with(|| "A proposta passou da validade de 7 dias.".to_owned());
    }
    Ok(Proposta {
        id,
        alteracao: de_json(&alteracao)?,
        origem,
        chave,
        criada_em,
        expira_em,
        estado,
        motivo,
    })
}

fn proposta_por(
    c: &Connection,
    coluna: &str,
    valor: &str,
    agora: i64,
) -> Resultado<Option<Proposta>> {
    c.query_row(
        &format!("SELECT {COLUNAS_DE_PROPOSTA} FROM propostas WHERE {coluna} = ?1"),
        params![valor],
        ler_linha_de_proposta,
    )
    .optional()?
    .map(|l| montar_proposta(l, agora))
    .transpose()
}

// ---------------------------------------------------------------------------------------
// O trait
// ---------------------------------------------------------------------------------------

impl Planejamento for PlanejamentoSqlite {
    fn controle(
        &self,
        periodo: &Periodo,
        filtro: Filtro,
        busca: Option<&str>,
        hoje: Data,
    ) -> Resultado<Controle> {
        let dados = carregar_dados(self.banco.conexao())?;
        let (inicio, fim) = (periodo.inicio(), periodo.fim());
        let (c_inicio, c_fim) = (Competencia::de_data(inicio), Competencia::de_data(fim));
        let c_antes = c_inicio.somar_meses(-MESES_DE_ATRASO);

        let mut linhas = Vec::new();
        let mut atrasadas = Vec::new();
        for rec in ativos(&dados) {
            let excecoes = dados.excecoes_de(&rec.id);
            let conciliacoes = dados.conciliacoes_de(&rec.id);
            let no_periodo = candidatas(c_inicio, c_fim, excecoes);
            linhas.extend(
                projetar_competencias_desde(
                    rec,
                    excecoes,
                    conciliacoes,
                    no_periodo,
                    hoje,
                    dados.inicio,
                )
                .into_iter()
                .filter(|o| inicio <= o.vencimento && o.vencimento <= fim),
            );
            let antes = candidatas(c_antes, c_inicio, excecoes);
            atrasadas.extend(
                projetar_competencias_desde(rec, excecoes, conciliacoes, antes, hoje, dados.inicio)
                    .into_iter()
                    .filter(|o| {
                        o.vencimento < inicio
                            && !o.suprimida()
                            && matches!(o.natureza, Natureza::Saida)
                            && o.devida_vencida(hoje)
                    }),
            );
        }

        // O resumo é do período inteiro, não do que o filtro ou a busca deixam ver.
        let visiveis: Vec<Ocorrencia> = linhas.iter().filter(|o| !o.suprimida()).cloned().collect();
        let resumo = resumo::calcular(&visiveis, &atrasadas, &self.saldos.saldos(), hoje);

        let busca = busca.map(normalizar).filter(|b| !b.trim().is_empty());
        let procura = |o: &Ocorrencia| {
            passa_no_filtro(o, filtro, hoje)
                && busca
                    .as_deref()
                    .is_none_or(|b| normalizar(&o.nome).contains(b.trim()))
        };
        linhas.retain(&procura);
        atrasadas.retain(&procura);
        ordenar(&mut linhas, &dados.grupos);
        ordenar(&mut atrasadas, &dados.grupos);

        Ok(Controle {
            periodo: *periodo,
            grupos: dados.grupos,
            linhas,
            atrasadas_de_antes: atrasadas,
            resumo,
        })
    }

    fn recorrentes(&self, incluir_excluidos: bool) -> Resultado<Vec<Recorrente>> {
        banco::ler_recorrentes(self.banco.conexao(), None, incluir_excluidos)
    }

    fn recorrente(&self, id: &str) -> Resultado<Option<Recorrente>> {
        Ok(
            banco::ler_recorrentes(self.banco.conexao(), Some(id), true)?
                .into_iter()
                .next(),
        )
    }

    fn grupos(&self) -> Resultado<Vec<Grupo>> {
        banco::ler_grupos(self.banco.conexao())
    }

    fn proximas(
        &self,
        recorrente: &str,
        desde: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<Ocorrencia>> {
        if meses > HORIZONTE_MESES {
            return Err(ErroDoPlanejamento::Invalida(format!(
                "o horizonte é de {HORIZONTE_MESES} meses"
            )));
        }
        if carregar_um(self.banco.conexao(), recorrente)?.is_none() {
            return Err(ErroDoPlanejamento::NaoEncontrado(format!(
                "recorrente {recorrente}"
            )));
        }
        let foto = fotografar(self.banco.conexao(), Some(recorrente), desde, meses, hoje)?;
        Ok(foto
            .ocorrencias
            .into_iter()
            .filter(|o| !o.suprimida())
            .collect())
    }

    fn calendario(
        &self,
        inicio: Competencia,
        meses: u16,
        hoje: Data,
    ) -> Resultado<Vec<TotaisDoMes>> {
        if meses > HORIZONTE_MESES {
            return Err(ErroDoPlanejamento::Invalida(format!(
                "o horizonte é de {HORIZONTE_MESES} meses"
            )));
        }
        let mut totais: Vec<TotaisDoMes> = (0..i32::from(meses))
            .map(|i| TotaisDoMes {
                competencia: inicio.somar_meses(i),
                entradas: Centavos(0),
                saidas_de_caixa: Centavos(0),
                descontos_em_folha: Centavos(0),
                sem_valor: 0,
                estimado: false,
            })
            .collect();
        if meses == 0 {
            return Ok(totais);
        }
        let ultimo = inicio.somar_meses(i32::from(meses) - 1);
        let dados = carregar_dados(self.banco.conexao())?;
        for rec in ativos(&dados) {
            let excecoes = dados.excecoes_de(&rec.id);
            let conciliacoes = dados.conciliacoes_de(&rec.id);
            let comps = candidatas(inicio, ultimo, excecoes);
            for o in
                projetar_competencias_desde(rec, excecoes, conciliacoes, comps, hoje, dados.inicio)
            {
                if o.suprimida() || o.estado == EstadoDaOcorrencia::Dispensada {
                    continue;
                }
                let Ok(indice) =
                    usize::try_from(inicio.meses_ate(Competencia::de_data(o.vencimento)))
                else {
                    continue;
                };
                let Some(mes) = totais.get_mut(indice) else {
                    continue;
                };
                let previsto = o.previsto.unwrap_or(Centavos(0));
                match o.natureza {
                    Natureza::Entrada => mes.entradas += previsto,
                    Natureza::Saida => mes.saidas_de_caixa += previsto,
                    Natureza::DescontoEmFolha => mes.descontos_em_folha += previsto,
                    Natureza::Passagem { .. } | Natureza::DentroDaFatura => {}
                }
                if o.previsto.is_none() {
                    mes.sem_valor += 1;
                }
                if o.e_incerta() {
                    mes.estimado = true;
                }
            }
        }
        Ok(totais)
    }

    fn simular(&self, alteracao: &Alteracao, hoje: Data) -> Resultado<Simulacao> {
        let tx = self.banco.conexao().unchecked_transaction()?;
        let inicio = inicio_da_simulacao(alteracao, hoje);
        let alvo = recorrente_do_alvo(&tx, alteracao)?;
        let antes = fotografar(&tx, alvo.as_deref(), inicio, MESES_DA_SIMULACAO, hoje)?;
        let recibo = aplicar_na(&tx, alteracao, &Autor::Usuario, &rfc3339(self.agora()))?;
        let alvo_depois = match alteracao {
            Alteracao::CriarRecorrente { .. } => Some(recibo.alvo),
            _ => alvo,
        };
        let depois = fotografar(
            &tx,
            alvo_depois.as_deref(),
            inicio,
            MESES_DA_SIMULACAO,
            hoje,
        )?;
        // Sempre desfaz: simular nunca grava.
        tx.rollback()?;

        let por_mes = |foto: &Foto| -> HashMap<Competencia, Ocorrencia> {
            foto.ocorrencias
                .iter()
                .map(|o| (o.id.competencia, o.clone()))
                .collect()
        };
        let (a, d) = (por_mes(&antes), por_mes(&depois));
        let mut simulacao = Simulacao {
            meses_afetados: Vec::new(),
            mudancas: Vec::new(),
            conflitos: conflitos_da(alteracao, inicio, &depois),
        };
        for i in 0..i32::from(MESES_DA_SIMULACAO) {
            let c = inicio.somar_meses(i);
            let (va, vd) = (vista(a.get(&c)), vista(d.get(&c)));
            if va != vd {
                simulacao.meses_afetados.push(c);
                let (pa, pd) = (va.and_then(|x| x.0), vd.and_then(|x| x.0));
                if pa != pd {
                    simulacao.mudancas.push((c, pa, pd));
                }
            }
        }
        Ok(simulacao)
    }

    fn aplicar(&mut self, alteracao: Alteracao, autor: Autor) -> Resultado<Recibo> {
        let agora = rfc3339(self.agora());
        let tx = self.banco.conexao_mut().transaction()?;
        let recibo = aplicar_na(&tx, &alteracao, &autor, &agora)?;
        tx.commit()?;
        Ok(recibo)
    }

    fn propor(&mut self, alteracao: Alteracao, origem: &str, chave: &str) -> Resultado<Proposta> {
        if chave.trim().is_empty() {
            return Err(ErroDoPlanejamento::Invalida(
                "a chave de idempotência não pode ficar vazia".into(),
            ));
        }
        let agora = self.agora();
        let tx = self.banco.conexao_mut().transaction()?;
        if let Some(existente) = proposta_por(&tx, "chave", chave, agora)? {
            return Ok(existente);
        }
        let id = novo_id(&tx, "prop")?;
        tx.execute(
            "INSERT INTO propostas (id, alteracao, origem, chave, criada_em, expira_em, estado)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pendente')",
            params![
                id,
                json(&alteracao)?,
                origem,
                chave,
                rfc3339(agora),
                rfc3339(agora + VALIDADE_DA_PROPOSTA_S)
            ],
        )?;
        let proposta = proposta_por(&tx, "id", &id, agora)?
            .ok_or_else(|| ErroDoPlanejamento::Banco("a proposta sumiu".into()))?;
        tx.commit()?;
        Ok(proposta)
    }

    fn propostas(&self, so_pendentes: bool) -> Resultado<Vec<Proposta>> {
        let agora = self.agora();
        let c = self.banco.conexao();
        let mut stmt = c.prepare(&format!(
            "SELECT {COLUNAS_DE_PROPOSTA} FROM propostas ORDER BY criada_em DESC, rowid DESC"
        ))?;
        let linhas = stmt
            .query_map([], ler_linha_de_proposta)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut propostas = linhas
            .into_iter()
            .map(|l| montar_proposta(l, agora))
            .collect::<Resultado<Vec<_>>>()?;
        if so_pendentes {
            propostas.retain(|p| p.estado == EstadoDaProposta::Pendente);
        }
        Ok(propostas)
    }

    fn confirmar_proposta(&mut self, id: &str) -> Resultado<Recibo> {
        let agora = self.agora();
        let texto_agora = rfc3339(agora);
        let mut tx = self.banco.conexao_mut().transaction()?;
        let proposta = proposta_por(&tx, "id", id, agora)?
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("proposta {id}")))?;
        match proposta.estado {
            EstadoDaProposta::Pendente => {}
            EstadoDaProposta::Expirada => {
                tx.execute(
                    "UPDATE propostas SET estado = 'expirada', motivo = COALESCE(motivo, ?2)
                     WHERE id = ?1 AND estado = 'pendente'",
                    params![id, proposta.motivo],
                )?;
                tx.commit()?;
                return Err(ErroDoPlanejamento::Invalida("a proposta expirou".into()));
            }
            _ => {
                return Err(ErroDoPlanejamento::Invalida(
                    "a proposta já foi decidida".into(),
                ));
            }
        }
        // A alteração roda num ponto de salvamento: se falhar, nada dela fica, e a proposta
        // continua pendente com o motivo da falha.
        let savepoint = tx.savepoint()?;
        match aplicar_na(
            &savepoint,
            &proposta.alteracao,
            &Autor::Usuario,
            &texto_agora,
        ) {
            Ok(recibo) => {
                savepoint.commit()?;
                tx.execute(
                    "UPDATE propostas SET estado = 'confirmada', motivo = NULL WHERE id = ?1",
                    params![id],
                )?;
                tx.commit()?;
                Ok(recibo)
            }
            Err(erro) => {
                drop(savepoint);
                tx.execute(
                    "UPDATE propostas SET motivo = ?2 WHERE id = ?1",
                    params![id, erro.to_string()],
                )?;
                tx.commit()?;
                Err(erro)
            }
        }
    }

    fn recusar_proposta(&mut self, id: &str, motivo: &str) -> Resultado<()> {
        if motivo.trim().is_empty() {
            return Err(ErroDoPlanejamento::Invalida(
                "o motivo da recusa não pode ficar vazio".into(),
            ));
        }
        let agora = self.agora();
        let tx = self.banco.conexao_mut().transaction()?;
        let proposta = proposta_por(&tx, "id", id, agora)?
            .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("proposta {id}")))?;
        if proposta.estado != EstadoDaProposta::Pendente {
            return Err(ErroDoPlanejamento::Invalida(
                "só se recusa proposta pendente".into(),
            ));
        }
        tx.execute(
            "UPDATE propostas SET estado = ?2, motivo = ?3 WHERE id = ?1",
            params![id, texto_do_estado(EstadoDaProposta::Recusada), motivo],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn historico(&self, alvo: Option<&str>, limite: u32) -> Resultado<Vec<EntradaDoHistorico>> {
        let c = self.banco.conexao();
        let mut stmt = c.prepare(
            "SELECT id, quando, autor, alvo, resumo, alteracao FROM historico
             WHERE (?1 IS NULL OR alvo = ?1 OR recorrente_id = ?1)
             ORDER BY sequencia DESC LIMIT ?2",
        )?;
        let brutas = stmt
            .query_map(params![alvo, limite], |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, String>(2)?,
                    l.get::<_, String>(3)?,
                    l.get::<_, String>(4)?,
                    l.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        brutas
            .into_iter()
            .map(|(id, quando, autor, alvo, resumo, alteracao)| {
                Ok(EntradaDoHistorico {
                    id,
                    quando,
                    autor: de_json(&autor)?,
                    alvo,
                    resumo,
                    alteracao,
                })
            })
            .collect()
    }

    fn sugestao_de_conciliacao(
        &self,
        ocorrencia: &crate::IdOcorrencia,
    ) -> Resultado<Option<Conciliacao>> {
        Ok(
            PlanejamentoSqlite::conciliacoes(self, &ocorrencia.recorrente)?
                .into_iter()
                .find(|c| !c.confirmada && &c.ocorrencia == ocorrencia),
        )
    }

    fn excecoes_do_recorrente(&self, recorrente: &str) -> Resultado<Vec<ExcecaoGravada>> {
        PlanejamentoSqlite::excecoes(self, recorrente)
    }

    fn origem_importada(&self, origem: &OrigemImportada) -> Resultado<Option<String>> {
        Ok(self
            .banco
            .conexao()
            .query_row(
                "SELECT recorrente_id FROM origens_importadas
                 WHERE sha256 = ?1 AND aba = ?2 AND linha = ?3",
                params![origem.arquivo_sha256, origem.aba, origem.linha],
                |l| l.get(0),
            )
            .optional()?)
    }

    fn composicao(
        &self,
        recorrente: &str,
        competencia: Competencia,
    ) -> Resultado<Option<Composicao>> {
        banco::ler_composicao(self.banco.conexao(), recorrente, competencia)
    }

    fn contas(&self) -> Resultado<Vec<ContaDoPlanejamento>> {
        extrato::contas(self)
    }

    fn pares_de_conta_sugeridos(&self) -> Resultado<Vec<ParDeConta>> {
        extrato::pares_sugeridos(self)
    }

    fn contas_do_extrato(&self) -> Resultado<Vec<String>> {
        Ok(extrato::contas_do_extrato(self))
    }

    fn assinatura(&self, recorrente: &str) -> Resultado<Option<Assinatura>> {
        extrato::assinatura(self, recorrente)
    }

    fn candidatos_de_conciliacao(
        &self,
        ocorrencia: &crate::IdOcorrencia,
    ) -> Resultado<Vec<MovimentoCandidato>> {
        extrato::candidatos(self, ocorrencia)
    }

    fn buscar_conciliacoes(&mut self, periodo: &Periodo, hoje: Data) -> Resultado<usize> {
        extrato::buscar(self, periodo, hoje)
    }

    fn conciliacoes_da_ocorrencia(
        &self,
        ocorrencia: &crate::IdOcorrencia,
    ) -> Resultado<Vec<crate::ConciliacaoComMovimento>> {
        extrato::conciliacoes_da_ocorrencia(self, ocorrencia)
    }

    /// Para uma ocorrência, devolve também os documentos da regra (o recorrente): um
    /// contrato ligado ao recorrente vale para todos os meses. Para o recorrente, só os da
    /// regra. Os mais recentes primeiro.
    fn documentos(&self, alvo: &AlvoDoDocumento) -> Resultado<Vec<DocumentoVinculado>> {
        let c = self.banco.conexao();
        match alvo {
            AlvoDoDocumento::Recorrente { id } => banco::ler_documentos(c, id, None),
            AlvoDoDocumento::Ocorrencia { id } => {
                banco::ler_documentos(c, &id.recorrente, Some(&id.to_string()))
            }
        }
    }
}

/// Atalho para quem lê conciliações de um recorrente (a importação e os testes).
impl PlanejamentoSqlite {
    /// O início do acompanhamento (data): o que vence (originalmente) antes dela e não tem
    /// pagamento informado não é "em atraso" (pago vazio é "não informado"). `None` = não definido.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn inicio_do_acompanhamento(&self) -> Resultado<Option<Data>> {
        banco::ler_inicio(self.banco.conexao())
    }

    /// Define o início do acompanhamento e deixa a linha no histórico (autor `Usuario`).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn definir_inicio_do_acompanhamento(&mut self, inicio: Data) -> Resultado<()> {
        let agora = rfc3339(self.agora());
        let tx = self.banco.conexao_mut().transaction()?;
        tx.execute(
            "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
             ON CONFLICT (chave) DO UPDATE SET valor = excluded.valor",
            params![banco::CHAVE_INICIO, inicio.to_string()],
        )?;
        let id = novo_id(&tx, "hist")?;
        tx.execute(
            "INSERT INTO historico (id, quando, autor, alvo, recorrente_id, resumo, alteracao)
             VALUES (?1, ?2, ?3, 'configuracao', NULL, ?4, ?5)",
            params![
                id,
                agora,
                json(&Autor::Usuario)?,
                format!("Definiu o início do acompanhamento em {inicio}."),
                json(&serde_json::json!({
                    "tipo": "definir_inicio_do_acompanhamento",
                    "inicio": inicio.to_string(),
                }))?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// As conciliações gravadas de um recorrente (confirmadas e sugestões).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn conciliacoes(&self, recorrente: &str) -> Resultado<Vec<Conciliacao>> {
        Ok(
            banco::ler_conciliacoes(self.banco.conexao(), Some(recorrente))?
                .remove(recorrente)
                .unwrap_or_default(),
        )
    }

    /// As exceções gravadas de um recorrente, inclusive as desfeitas.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn excecoes(&self, recorrente: &str) -> Resultado<Vec<ExcecaoGravada>> {
        Ok(banco::ler_excecoes(self.banco.conexao(), Some(recorrente))?
            .remove(recorrente)
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod testes {
    use super::normalizar;

    #[test]
    fn busca_ignora_acento_e_caixa() {
        assert_eq!(normalizar("Empréstimo Exemplo"), "emprestimo exemplo");
        assert_eq!(normalizar("CONSIGNAÇÃO"), "consignacao");
    }
}
