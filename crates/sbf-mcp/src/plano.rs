//! As ferramentas do controle mensal (spec 11, AC-30). Falam só com o trait
//! [`Planejamento`]: leem, simulam e **propõem**. Nunca chamam `aplicar`,
//! `confirmar_proposta` ou `recusar_proposta`; quem decide é a janela do SBF.
//!
//! Cada chamada abre uma instância nova do serviço (pela [`Fabrica`]), roda, passa o texto
//! livre pelo nível de confiança do cliente e grava uma linha em `saida_ia` (destinatário
//! `mcp:<ferramenta>`), sem o conteúdo. Nada de valor ou nome vai a log.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::schemars;
use sbf_ia::NivelDeConfianca;
use sbf_planejamento::{
    Alteracao, Centavos, Certeza, Ciclo, Competencia, Conciliacao, Data, DiaDeCorte,
    ErroDoPlanejamento, Escopo, Excecao, Filtro, Fonte, Frequencia, HORIZONTE_MESES, IdOcorrencia,
    MudancaDeVersao, Natureza, NovoRecorrente, Periodo, Planejamento, Prazo, Sentido, Versao,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Abre uma instância do serviço de planejamento (uma por chamada).
pub type Fabrica = dyn Fn() -> Result<Box<dyn Planejamento>, String> + Send + Sync;

/// O máximo de itens por página.
pub const POR_PAGINA_MAXIMO: usize = 100;
const POR_PAGINA_PADRAO: usize = 25;
/// O máximo de meses de `projetar_compromissos`.
pub const MESES_MAXIMOS: u16 = HORIZONTE_MESES;
const MAX_NOME: usize = 120;
const MAX_MOTIVO: usize = 200;
const MAX_NOTA: usize = 500;
const MAX_CHAVE: usize = 120;
const MAX_BUSCA: usize = 100;
/// O maior valor aceito, em centavos (1 bilhão de reais).
const MAX_CENTAVOS: i64 = 100_000_000_000;
/// A origem das propostas deste servidor.
pub const ORIGEM: &str = "mcp";
/// O que o Claude lê depois de propor.
pub const AVISO_DE_PROPOSTA: &str = "proposta pendente: confirme na janela do SBF";

/// Os nomes das ferramentas do controle mensal.
pub const FERRAMENTAS: [&str; 14] = [
    "listar_compromissos",
    "resumo_de_caixa",
    "projetar_compromissos",
    "obter_compromisso",
    "simular_mudanca",
    "listar_propostas",
    "propor_compromisso",
    "propor_alteracao",
    "propor_pausa",
    "propor_retomada",
    "propor_cancelamento",
    "propor_exclusao_logica",
    "propor_conciliacao",
    "propor_excecao",
];

type Resultado = Result<Value, String>;

fn pt(e: ErroDoPlanejamento) -> String {
    match e {
        ErroDoPlanejamento::VersaoVelha { atual } => format!(
            "o compromisso mudou desde a sua consulta (versão atual {atual}): consulte de novo e refaça"
        ),
        ErroDoPlanejamento::NaoEncontrado(o) => format!("não encontrei: {o}"),
        ErroDoPlanejamento::Invalida(m) => format!("alteração inválida: {m}"),
        ErroDoPlanejamento::Banco(_) => "falha ao consultar o planejamento".to_owned(),
    }
}

// ---------------------------------------------------------------------------------------
// Pedidos (os esquemas que o Claude vê)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeLista {
    /// "ciclo" (padrão, corte no dia 20) ou "mes" (mês civil).
    pub periodo: Option<String>,
    /// AAAA-MM do ciclo (o mês em que ele começa) ou do mês. Padrão: o de hoje.
    pub referencia: Option<String>,
    /// todos (padrão), a_pagar, em_atraso, pagos, sem_comprovante, sem_valor ou pausados_e_cancelados.
    pub filtro: Option<String>,
    /// Trecho do nome, do grupo, da conta ou do fornecedor (até 100 caracteres).
    pub busca: Option<String>,
    /// Página, a partir de 1 (padrão 1).
    pub pagina: Option<u32>,
    /// Itens por página, de 1 a 100 (padrão 25).
    pub por_pagina: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDePeriodo {
    /// "ciclo" (padrão) ou "mes".
    pub periodo: Option<String>,
    /// AAAA-MM. Padrão: o de hoje.
    pub referencia: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeProjecao {
    /// AAAA-MM do primeiro mês. Padrão: o mês de hoje.
    pub inicio: Option<String>,
    /// Quantos meses, de 1 a 360 (padrão 12).
    pub meses: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeCompromisso {
    /// O id de uma ocorrência ("rec:AAAA-MM" ou "rec:AAAA-MM:pN") ou o id do recorrente.
    pub id_ocorrencia: String,
    /// Quantos meses de próximas ocorrências, de 1 a 60 (padrão 12).
    pub proximas: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDePropostas {
    /// true (padrão): só as pendentes.
    pub so_pendentes: Option<bool>,
    /// Página, a partir de 1 (padrão 1).
    pub pagina: Option<u32>,
    /// Itens por página, de 1 a 100 (padrão 25).
    pub por_pagina: Option<u32>,
}

/// Um compromisso novo. Valores em centavos; sem `valor_centavos` o valor fica "a conferir".
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoNovoCompromisso {
    pub nome: String,
    /// Grupo de nível 1 (Essencial, Lar...).
    pub grupo: String,
    pub subgrupo: Option<String>,
    /// Posição dentro do grupo (menor vem antes; padrão 1000).
    pub posicao: Option<i64>,
    /// entrada, saida, desconto_em_folha, passagem_entrada ou passagem_saida.
    pub natureza: String,
    /// Valor em centavos, maior ou igual a zero. Sem ele, fica sem valor.
    pub valor_centavos: Option<i64>,
    /// confirmado ou estimado (padrão).
    pub certeza: Option<String>,
    /// AAAA-MM da primeira competência.
    pub desde: String,
    /// mensal (padrão), cada_n_meses, anual ou unica.
    pub frequencia: Option<String>,
    /// Dia do vencimento, de 1 a 31 (obrigatório menos em "unica").
    pub dia: Option<u8>,
    /// Para cada_n_meses: o intervalo, de 2 a 60.
    pub a_cada_n_meses: Option<u8>,
    /// Para anual: o mês, de 1 a 12.
    pub mes_do_ano: Option<u8>,
    /// Para unica: AAAA-MM-DD.
    pub data: Option<String>,
    /// sem_fim (padrão), desconhecido, parcelas ou ate.
    pub prazo: Option<String>,
    /// Para prazo parcelas: quantas, de 1 a 600.
    pub parcelas: Option<u32>,
    /// Para prazo ate: AAAA-MM da primeira competência que já não ocorre.
    pub ate_competencia: Option<String>,
    pub conta: Option<String>,
    pub centro: Option<String>,
    pub fornecedor: Option<String>,
    pub categoria: Option<String>,
    pub nota: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeAlteracao {
    /// O id do recorrente (veja obter_compromisso).
    pub recorrente: String,
    /// so_este_mes (só o valor, só nesta competência) ou daqui_em_diante (versão nova).
    pub escopo: String,
    /// AAAA-MM: o mês (so_este_mes) ou o primeiro mês da versão nova (daqui_em_diante).
    pub competencia: String,
    /// Novo valor em centavos, maior ou igual a zero.
    pub valor_centavos: Option<i64>,
    /// true: passa a "sem valor" (a conferir). Só em daqui_em_diante.
    pub sem_valor: Option<bool>,
    /// confirmado ou estimado. Só em daqui_em_diante.
    pub certeza: Option<String>,
    pub conta: Option<String>,
    pub centro: Option<String>,
    pub fornecedor: Option<String>,
    pub categoria: Option<String>,
    pub nota: Option<String>,
    /// Campos a limpar: conta, centro, fornecedor, categoria, nota. Só em daqui_em_diante.
    pub limpar: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDePausa {
    pub recorrente: String,
    /// AAAA-MM: primeiro mês pausado.
    pub desde: String,
    /// AAAA-MM exclusivo. Sem ele, pausa até retomar.
    pub ate: Option<String>,
    pub motivo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeRetomada {
    pub recorrente: String,
    /// AAAA-MM: primeiro mês que volta a ocorrer.
    pub desde: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeCancelamento {
    pub recorrente: String,
    /// AAAA-MM: primeiro mês sem ocorrência.
    pub desde: String,
    pub motivo: String,
    /// true só se há documento do fornecedor (padrão false).
    pub comprovado: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeExclusao {
    pub recorrente: String,
    pub motivo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeConciliacao {
    /// "rec:AAAA-MM" ou "rec:AAAA-MM:pN".
    pub id_ocorrencia: String,
    /// O id do movimento (`pluggy:txn_<id>` ou id da movimentação).
    pub movimento: String,
    pub valor_centavos: i64,
    /// AAAA-MM-DD do movimento.
    pub data: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeExcecao {
    /// "rec:AAAA-MM" ou "rec:AAAA-MM:pN".
    pub id_ocorrencia: String,
    /// adiar, dispensar, pago_informado ou valor_so_deste_mes.
    pub acao: String,
    /// Para adiar: AAAA-MM-DD. Sem ela, adia sem data.
    pub nova_data: Option<String>,
    /// Para dispensar: por quê.
    pub motivo: Option<String>,
    /// Para pago_informado e valor_so_deste_mes: o valor em centavos.
    pub valor_centavos: Option<i64>,
    /// Para pago_informado: AAAA-MM-DD do pagamento.
    pub data: Option<String>,
}

/// Qualquer alteração, para `simular_mudanca`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EntradaDeAlteracao {
    Compromisso(PedidoNovoCompromisso),
    Alteracao(PedidoDeAlteracao),
    Pausa(PedidoDePausa),
    Retomada(PedidoDeRetomada),
    Cancelamento(PedidoDeCancelamento),
    ExclusaoLogica(PedidoDeExclusao),
    Conciliacao(PedidoDeConciliacao),
    Excecao(PedidoDeExcecao),
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeSimulacao {
    /// A mudança, com "tipo": compromisso, alteracao, pausa, retomada, cancelamento,
    /// exclusao_logica, conciliacao ou excecao, mais os campos de cada uma.
    pub alteracao: EntradaDeAlteracao,
}

/// Um pedido de proposta: a chave de idempotência mais os campos da mudança.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ComChave<T> {
    /// Chave de idempotência (até 120 caracteres): a mesma chave devolve a mesma proposta.
    pub chave: String,
    #[serde(flatten)]
    pub pedido: T,
}

// ---------------------------------------------------------------------------------------
// Validação
// ---------------------------------------------------------------------------------------

fn competencia(t: &str, campo: &str) -> Result<Competencia, String> {
    Competencia::de_texto(t.trim())
        .ok_or_else(|| format!("{campo} deve ser AAAA-MM (por exemplo 2026-11)"))
}

fn data(t: &str, campo: &str) -> Result<Data, String> {
    Data::de_iso(t.trim()).ok_or_else(|| format!("{campo} deve ser AAAA-MM-DD"))
}

/// Texto obrigatório, aparado, de 1 a `max` caracteres.
fn texto(t: &str, campo: &str, max: usize) -> Result<String, String> {
    let t = t.trim();
    if t.is_empty() {
        return Err(format!("{campo} não pode ficar vazio"));
    }
    if t.chars().count() > max {
        return Err(format!("{campo} passa de {max} caracteres"));
    }
    Ok(t.to_owned())
}

fn texto_opcional(t: &Option<String>, campo: &str, max: usize) -> Result<Option<String>, String> {
    t.as_deref().map(|t| texto(t, campo, max)).transpose()
}

fn centavos(v: i64, campo: &str) -> Result<Centavos, String> {
    if !(0..=MAX_CENTAVOS).contains(&v) {
        return Err(format!(
            "{campo} deve estar entre 0 e {MAX_CENTAVOS} centavos"
        ));
    }
    Ok(Centavos(v))
}

fn pagina(pagina: Option<u32>, por_pagina: Option<u32>) -> Result<(usize, usize), String> {
    let pg = pagina.unwrap_or(1);
    if pg == 0 {
        return Err("pagina começa em 1".to_owned());
    }
    let por = por_pagina.map_or(POR_PAGINA_PADRAO, |v| v as usize);
    if !(1..=POR_PAGINA_MAXIMO).contains(&por) {
        return Err(format!(
            "por_pagina deve estar entre 1 e {POR_PAGINA_MAXIMO}"
        ));
    }
    Ok((pg as usize, por))
}

fn filtro(t: &Option<String>) -> Result<Filtro, String> {
    Ok(match t.as_deref().map(str::trim).unwrap_or("todos") {
        "todos" => Filtro::Todos,
        "a_pagar" => Filtro::APagar,
        "em_atraso" => Filtro::EmAtraso,
        "pagos" => Filtro::Pagos,
        "sem_comprovante" => Filtro::SemComprovante,
        "sem_valor" => Filtro::SemValor,
        "pausados_e_cancelados" => Filtro::PausadosECancelados,
        _ => {
            return Err("filtro deve ser: todos, a_pagar, em_atraso, pagos, sem_comprovante, sem_valor ou pausados_e_cancelados".to_owned())
        }
    })
}

fn certeza(t: &str) -> Result<Certeza, String> {
    match t.trim() {
        "confirmado" => Ok(Certeza::Confirmado),
        "estimado" => Ok(Certeza::Estimado),
        _ => Err("certeza deve ser confirmado ou estimado".to_owned()),
    }
}

fn natureza(t: &str) -> Result<Natureza, String> {
    Ok(match t.trim() {
        "entrada" => Natureza::Entrada,
        "saida" => Natureza::Saida,
        "desconto_em_folha" => Natureza::DescontoEmFolha,
        "dentro_da_fatura" => Natureza::DentroDaFatura,
        "passagem_entrada" => Natureza::Passagem {
            sentido: Sentido::Entrada,
        },
        "passagem_saida" => Natureza::Passagem {
            sentido: Sentido::Saida,
        },
        _ => {
            return Err("natureza deve ser: entrada, saida, desconto_em_folha, passagem_entrada ou passagem_saida".to_owned())
        }
    })
}

fn dia_do_mes(v: Option<u8>) -> Result<u8, String> {
    match v {
        Some(d) if (1..=31).contains(&d) => Ok(d),
        _ => Err("dia deve estar entre 1 e 31".to_owned()),
    }
}

fn id_de_ocorrencia(t: &str) -> Result<IdOcorrencia, String> {
    IdOcorrencia::de_texto(t.trim())
        .ok_or_else(|| "id_ocorrencia deve ser rec:AAAA-MM ou rec:AAAA-MM:pN".to_owned())
}

fn id_do_recorrente(t: &str) -> Result<String, String> {
    texto(t, "recorrente", MAX_NOME)
}

/// A versão atual do registro, para a concorrência otimista (AC-31 na confirmação).
fn versao_base(p: &dyn Planejamento, id: &str) -> Result<i64, String> {
    match p.recorrente(id).map_err(pt)? {
        Some(r) => Ok(r.versao_do_registro),
        None => {
            Err("não encontrei esse compromisso (confira o id com obter_compromisso)".to_owned())
        }
    }
}

fn montar_novo(p: &PedidoNovoCompromisso) -> Result<Alteracao, String> {
    let desde = competencia(&p.desde, "desde")?;
    let nome = texto(&p.nome, "nome", MAX_NOME)?;
    let grupo = texto(&p.grupo, "grupo", MAX_NOME)?;
    let subgrupo = texto_opcional(&p.subgrupo, "subgrupo", MAX_NOME)?;
    let posicao = p.posicao.unwrap_or(1000);
    if !(0..=100_000).contains(&posicao) {
        return Err("posicao deve estar entre 0 e 100000".to_owned());
    }
    let frequencia = match p.frequencia.as_deref().map(str::trim).unwrap_or("mensal") {
        "mensal" => Frequencia::Mensal {
            dia: dia_do_mes(p.dia)?,
        },
        "cada_n_meses" => {
            let n = p.a_cada_n_meses.unwrap_or(0);
            if !(2..=60).contains(&n) {
                return Err("a_cada_n_meses deve estar entre 2 e 60".to_owned());
            }
            Frequencia::CadaNMeses {
                n,
                dia: dia_do_mes(p.dia)?,
                primeira: desde,
            }
        }
        "anual" => {
            let mes = p.mes_do_ano.unwrap_or(0);
            if !(1..=12).contains(&mes) {
                return Err("mes_do_ano deve estar entre 1 e 12".to_owned());
            }
            Frequencia::Anual {
                mes,
                dia: dia_do_mes(p.dia)?,
            }
        }
        "unica" => Frequencia::Unica {
            data: data(p.data.as_deref().unwrap_or(""), "data")?,
        },
        _ => return Err("frequencia deve ser: mensal, cada_n_meses, anual ou unica".to_owned()),
    };
    let prazo = match p.prazo.as_deref().map(str::trim).unwrap_or("sem_fim") {
        "sem_fim" => Prazo::SemFim,
        "desconhecido" => Prazo::Desconhecido,
        "parcelas" => {
            let total = p.parcelas.unwrap_or(0);
            if !(1..=600).contains(&total) {
                return Err("parcelas deve estar entre 1 e 600".to_owned());
            }
            Prazo::Parcelas {
                total,
                primeira: desde,
            }
        }
        "ate" => {
            let ate = competencia(
                p.ate_competencia.as_deref().unwrap_or(""),
                "ate_competencia",
            )?;
            if ate <= desde {
                return Err("ate_competencia deve vir depois de desde".to_owned());
            }
            Prazo::AteCompetencia(ate)
        }
        _ => return Err("prazo deve ser: sem_fim, desconhecido, parcelas ou ate".to_owned()),
    };
    let valor = p
        .valor_centavos
        .map(|v| centavos(v, "valor_centavos"))
        .transpose()?;
    Ok(Alteracao::CriarRecorrente {
        recorrente: NovoRecorrente {
            nome,
            grupo,
            subgrupo,
            posicao,
            natureza: natureza(&p.natureza)?,
            prazo,
            primeira_versao: Versao {
                id: String::new(),
                desde,
                valor,
                certeza: p
                    .certeza
                    .as_deref()
                    .map_or(Ok(Certeza::Estimado), certeza)?,
                fonte: Fonte::Ia,
                frequencia,
                conta: texto_opcional(&p.conta, "conta", MAX_NOME)?,
                centro: texto_opcional(&p.centro, "centro", MAX_NOME)?,
                fornecedor: texto_opcional(&p.fornecedor, "fornecedor", MAX_NOME)?,
                categoria: texto_opcional(&p.categoria, "categoria", MAX_NOME)?,
                nota: texto_opcional(&p.nota, "nota", MAX_NOTA)?,
            },
        },
        origem: None,
    })
}

fn montar_alteracao(pl: &dyn Planejamento, p: &PedidoDeAlteracao) -> Result<Alteracao, String> {
    let rec = id_do_recorrente(&p.recorrente)?;
    let comp = competencia(&p.competencia, "competencia")?;
    let mut m = MudancaDeVersao::default();
    if p.valor_centavos.is_some() && p.sem_valor == Some(true) {
        return Err("use valor_centavos ou sem_valor, não os dois".to_owned());
    }
    if let Some(v) = p.valor_centavos {
        m.valor = Some(Some(centavos(v, "valor_centavos")?));
    } else if p.sem_valor == Some(true) {
        m.valor = Some(None);
    }
    if let Some(c) = &p.certeza {
        m.certeza = Some(certeza(c)?);
    }
    m.conta = texto_opcional(&p.conta, "conta", MAX_NOME)?.map(Some);
    m.centro = texto_opcional(&p.centro, "centro", MAX_NOME)?.map(Some);
    m.fornecedor = texto_opcional(&p.fornecedor, "fornecedor", MAX_NOME)?.map(Some);
    m.categoria = texto_opcional(&p.categoria, "categoria", MAX_NOME)?.map(Some);
    m.nota = texto_opcional(&p.nota, "nota", MAX_NOTA)?.map(Some);
    for campo in p.limpar.iter().flatten() {
        let alvo = match campo.trim() {
            "conta" => &mut m.conta,
            "centro" => &mut m.centro,
            "fornecedor" => &mut m.fornecedor,
            "categoria" => &mut m.categoria,
            "nota" => &mut m.nota,
            _ => return Err("limpar aceita: conta, centro, fornecedor, categoria, nota".to_owned()),
        };
        if alvo.is_some() {
            return Err(format!("{campo} não pode ser dado e limpo ao mesmo tempo"));
        }
        *alvo = Some(None);
    }
    if m == MudancaDeVersao::default() {
        return Err("diga o que mudar (valor_centavos, certeza, conta...)".to_owned());
    }
    let escopo = match p.escopo.trim() {
        "so_este_mes" => {
            let so_valor = MudancaDeVersao {
                valor: m.valor,
                ..MudancaDeVersao::default()
            };
            if !matches!(m.valor, Some(Some(_))) || m != so_valor {
                return Err("so_este_mes muda apenas o valor (valor_centavos); para o resto use daqui_em_diante".to_owned());
            }
            Escopo::SoEsteMes(comp)
        }
        "daqui_em_diante" => Escopo::DaquiEmDiante(comp),
        _ => return Err("escopo deve ser so_este_mes ou daqui_em_diante".to_owned()),
    };
    Ok(Alteracao::Editar {
        versao_base: versao_base(pl, &rec)?,
        recorrente: rec,
        escopo,
        mudanca: m,
    })
}

fn montar(pl: &dyn Planejamento, e: &EntradaDeAlteracao) -> Result<Alteracao, String> {
    match e {
        EntradaDeAlteracao::Compromisso(p) => montar_novo(p),
        EntradaDeAlteracao::Alteracao(p) => montar_alteracao(pl, p),
        EntradaDeAlteracao::Pausa(p) => {
            let rec = id_do_recorrente(&p.recorrente)?;
            let desde = competencia(&p.desde, "desde")?;
            let ate = p
                .ate
                .as_deref()
                .map(|a| competencia(a, "ate"))
                .transpose()?;
            if ate.is_some_and(|a| a <= desde) {
                return Err("ate deve vir depois de desde".to_owned());
            }
            Ok(Alteracao::Pausar {
                versao_base: versao_base(pl, &rec)?,
                recorrente: rec,
                desde,
                ate,
                motivo: texto(&p.motivo, "motivo", MAX_MOTIVO)?,
            })
        }
        EntradaDeAlteracao::Retomada(p) => {
            let rec = id_do_recorrente(&p.recorrente)?;
            Ok(Alteracao::Retomar {
                versao_base: versao_base(pl, &rec)?,
                recorrente: rec,
                desde: competencia(&p.desde, "desde")?,
            })
        }
        EntradaDeAlteracao::Cancelamento(p) => {
            let rec = id_do_recorrente(&p.recorrente)?;
            Ok(Alteracao::Cancelar {
                versao_base: versao_base(pl, &rec)?,
                recorrente: rec,
                desde: competencia(&p.desde, "desde")?,
                motivo: texto(&p.motivo, "motivo", MAX_MOTIVO)?,
                comprovado: p.comprovado.unwrap_or(false),
            })
        }
        EntradaDeAlteracao::ExclusaoLogica(p) => {
            let rec = id_do_recorrente(&p.recorrente)?;
            Ok(Alteracao::Excluir {
                versao_base: versao_base(pl, &rec)?,
                recorrente: rec,
                motivo: texto(&p.motivo, "motivo", MAX_MOTIVO)?,
            })
        }
        EntradaDeAlteracao::Conciliacao(p) => Ok(Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: id_de_ocorrencia(&p.id_ocorrencia)?,
                movimento: texto(&p.movimento, "movimento", MAX_NOME)?,
                valor: centavos(p.valor_centavos, "valor_centavos")?,
                data: data(&p.data, "data")?,
                // A IA só sugere; confirmar a conciliação é da janela.
                confirmada: false,
            },
        }),
        EntradaDeAlteracao::Excecao(p) => {
            let ocorrencia = id_de_ocorrencia(&p.id_ocorrencia)?;
            let excecao = match p.acao.trim() {
                "adiar" => Excecao::Adiar {
                    nova_data: p
                        .nova_data
                        .as_deref()
                        .map(|d| data(d, "nova_data"))
                        .transpose()?,
                },
                "dispensar" => Excecao::Dispensar {
                    motivo: texto(p.motivo.as_deref().unwrap_or(""), "motivo", MAX_MOTIVO)?,
                },
                "pago_informado" => Excecao::PagoInformado {
                    valor: centavos(
                        p.valor_centavos
                            .ok_or("pago_informado pede valor_centavos")?,
                        "valor_centavos",
                    )?,
                    data: p.data.as_deref().map(|d| data(d, "data")).transpose()?,
                },
                "valor_so_deste_mes" => Excecao::ValorSoDesteMes {
                    valor: centavos(
                        p.valor_centavos
                            .ok_or("valor_so_deste_mes pede valor_centavos")?,
                        "valor_centavos",
                    )?,
                },
                _ => {
                    return Err(
                        "acao deve ser: adiar, dispensar, pago_informado ou valor_so_deste_mes"
                            .to_owned(),
                    );
                }
            };
            Ok(Alteracao::Excecao {
                ocorrencia,
                excecao,
            })
        }
    }
}

// ---------------------------------------------------------------------------------------
// Saída: nível de confiança e formato
// ---------------------------------------------------------------------------------------

/// Os campos de texto livre (nomes, notas, motivos): passam pelo nível de confiança.
const CHAVES_LIVRES: [&str; 12] = [
    "nome",
    "grupo",
    "subgrupo",
    "conta",
    "centro",
    "fornecedor",
    "categoria",
    "nota",
    "motivo",
    "resumo",
    "conflitos",
    "lacunas",
];

fn eh_competencia(m: &Map<String, Value>) -> Option<String> {
    if m.len() != 2 {
        return None;
    }
    let ano = m.get("ano")?.as_i64()?;
    let mes = m.get("mes")?.as_u64()?;
    Some(format!("{ano:04}-{mes:02}"))
}

/// Mascara o texto livre e deixa as datas e os ids legíveis (`{ano, mes}` vira `AAAA-MM`;
/// um id de ocorrência vira `rec:AAAA-MM[:pN]`).
fn tratar(v: Value, nivel: NivelDeConfianca, livre: bool) -> Value {
    match v {
        Value::String(s) if livre => Value::String(sbf_indice::perguntar::mascarar(&s, nivel)),
        Value::Array(a) => Value::Array(a.into_iter().map(|x| tratar(x, nivel, livre)).collect()),
        Value::Object(m) => {
            let mut novo = Map::new();
            for (k, x) in m {
                let l = CHAVES_LIVRES.contains(&k.as_str());
                novo.insert(k, tratar(x, nivel, l));
            }
            if let Some(c) = eh_competencia(&novo) {
                return Value::String(c);
            }
            if let (Some(Value::String(r)), Some(Value::String(c))) =
                (novo.get("recorrente"), novo.get("competencia"))
                && novo
                    .keys()
                    .all(|k| matches!(k.as_str(), "recorrente" | "competencia" | "parcela"))
            {
                let mut id = format!("{r}:{c}");
                if let Some(n) = novo.get("parcela").and_then(Value::as_u64) {
                    id.push_str(&format!(":p{n}"));
                }
                return Value::String(id);
            }
            Value::Object(novo)
        }
        outro => outro,
    }
}

fn periodo_json(p: &Periodo) -> Value {
    json!({
        "tipo": match p { Periodo::Ciclo(_) => "ciclo", Periodo::MesCivil(_) => "mes" },
        "inicio": p.inicio().to_string(),
        "fim": p.fim().to_string(),
    })
}

fn simulacao_json(s: &sbf_planejamento::Simulacao) -> Value {
    json!({
        "meses_afetados": s.meses_afetados.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "mudancas": s.mudancas.iter().map(|(c, antes, depois)| json!({
            "competencia": c.to_string(),
            "antes_centavos": antes.map(|v| v.0),
            "depois_centavos": depois.map(|v| v.0),
        })).collect::<Vec<_>>(),
        "conflitos": s.conflitos,
    })
}

fn ok_serde<T: serde::Serialize>(v: &T) -> Resultado {
    serde_json::to_value(v).map_err(|_| "falha ao montar a resposta".to_owned())
}

// ---------------------------------------------------------------------------------------
// O núcleo do planejamento
// ---------------------------------------------------------------------------------------

/// O que as ferramentas do controle mensal usam: a fábrica do serviço, o inventário (para
/// `saida_ia`) e o nível de confiança deste cliente.
#[derive(Clone)]
pub struct Plano {
    pub(crate) fabrica: Arc<Fabrica>,
    pub(crate) banco: PathBuf,
    pub(crate) nivel: NivelDeConfianca,
    pub(crate) hoje: Option<Data>,
}

impl fmt::Debug for Plano {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plano").finish_non_exhaustive()
    }
}

impl Plano {
    fn hoje(&self) -> Data {
        self.hoje.unwrap_or_else(Data::hoje_em_sao_paulo)
    }

    /// Abre o serviço, roda, trata o texto no nível do cliente e registra a saída.
    fn executar(
        &self,
        ferramenta: &str,
        f: impl FnOnce(&mut dyn Planejamento, Data) -> Resultado,
    ) -> Resultado {
        let r = (self.fabrica)().and_then(|mut p| f(p.as_mut(), self.hoje()));
        let r = r.map(|v| tratar(v, self.nivel, false));
        let bytes = match &r {
            Ok(v) => v.to_string().len(),
            Err(e) => e.len(),
        } as u64;
        self.registrar(ferramenta, bytes)?;
        r
    }

    fn registrar(&self, ferramenta: &str, bytes: u64) -> Result<(), String> {
        use sbf_ia::Registro as _;
        let registro = sbf_inventario::sugestao::RegistroDaIa::abrir(&self.banco, None)
            .map_err(|_| "não deu para registrar a saída".to_owned())?;
        registro
            .registrar_saida(&sbf_ia::Saida {
                destinatario: format!("mcp:{ferramenta}"),
                provedor: "mcp".to_owned(),
                modelo: "-".to_owned(),
                nivel: self.nivel,
                bytes_enviados: bytes,
                consumo: sbf_ia::Consumo::default(),
                quando: sbf_ia::agora_rfc3339(),
            })
            .map_err(|_| "não deu para registrar a saída".to_owned())
    }

    fn periodo(
        periodo: &Option<String>,
        referencia: &Option<String>,
        hoje: Data,
    ) -> Result<Periodo, String> {
        let ref_ = referencia
            .as_deref()
            .map(|r| competencia(r, "referencia"))
            .transpose()?;
        match periodo.as_deref().map(str::trim).unwrap_or("ciclo") {
            "ciclo" => Ok(Periodo::Ciclo(match ref_ {
                Some(c) => Ciclo::do_mes(c.ano, c.mes, DiaDeCorte::PADRAO),
                None => Ciclo::da_data(hoje, DiaDeCorte::PADRAO),
            })),
            "mes" => Ok(Periodo::MesCivil(
                ref_.unwrap_or_else(|| Competencia::de_data(hoje)),
            )),
            _ => Err("periodo deve ser ciclo ou mes".to_owned()),
        }
    }

    /// Os compromissos do período, paginados.
    ///
    /// # Errors
    /// Argumento fora dos limites ou falha do serviço.
    pub fn listar_compromissos(&self, a: &PedidoDeLista) -> Resultado {
        self.executar("listar_compromissos", |p, hoje| {
            let periodo = Self::periodo(&a.periodo, &a.referencia, hoje)?;
            let f = filtro(&a.filtro)?;
            let busca = texto_opcional(&a.busca, "busca", MAX_BUSCA)?;
            let (pg, por) = pagina(a.pagina, a.por_pagina)?;
            let c = p
                .controle(&periodo, f, busca.as_deref(), hoje)
                .map_err(pt)?;
            let total = c.linhas.len();
            let itens: Vec<_> = c.linhas.iter().skip((pg - 1) * por).take(por).collect();
            let mut v = json!({
                "periodo": periodo_json(&periodo),
                "filtro": a.filtro.as_deref().unwrap_or("todos"),
                "total": total,
                "pagina": pg,
                "por_pagina": por,
                "total_de_paginas": total.div_ceil(por),
                "itens": ok_serde(&itens)?,
                "valores": "em centavos",
            });
            if pg == 1 {
                v["atrasadas_de_antes"] = ok_serde(&c.atrasadas_de_antes)?;
            }
            Ok(v)
        })
    }

    /// Os números do topo do controle mensal.
    ///
    /// # Errors
    /// Argumento inválido ou falha do serviço.
    pub fn resumo_de_caixa(&self, a: &PedidoDePeriodo) -> Resultado {
        self.executar("resumo_de_caixa", |p, hoje| {
            let periodo = Self::periodo(&a.periodo, &a.referencia, hoje)?;
            let c = p
                .controle(&periodo, Filtro::Todos, None, hoje)
                .map_err(pt)?;
            Ok(json!({
                "periodo": periodo_json(&periodo),
                "resumo": ok_serde(&c.resumo)?,
                "linhas": c.linhas.len(),
                "atrasadas_de_antes": c.atrasadas_de_antes.len(),
                "valores": "em centavos",
            }))
        })
    }

    /// Os totais mês a mês, até 360 meses.
    ///
    /// # Errors
    /// Argumento fora dos limites ou falha do serviço.
    pub fn projetar_compromissos(&self, a: &PedidoDeProjecao) -> Resultado {
        self.executar("projetar_compromissos", |p, hoje| {
            let inicio = match &a.inicio {
                Some(t) => competencia(t, "inicio")?,
                None => Competencia::de_data(hoje),
            };
            let meses = a.meses.unwrap_or(12);
            if !(1..=u32::from(MESES_MAXIMOS)).contains(&meses) {
                return Err(format!("meses deve estar entre 1 e {MESES_MAXIMOS}"));
            }
            let totais = p
                .calendario(inicio, u16::try_from(meses).unwrap_or(MESES_MAXIMOS), hoje)
                .map_err(pt)?;
            let soma = |f: fn(&sbf_planejamento::TotaisDoMes) -> i64| {
                totais.iter().fold(0_i64, |s, t| s.saturating_add(f(t)))
            };
            Ok(json!({
                "inicio": inicio.to_string(),
                "meses": totais.len(),
                "soma": {
                    "entradas_centavos": soma(|t| t.entradas.0),
                    "saidas_de_caixa_centavos": soma(|t| t.saidas_de_caixa.0),
                    "descontos_em_folha_centavos": soma(|t| t.descontos_em_folha.0),
                },
                "totais": ok_serde(&totais)?,
                "valores": "em centavos",
            }))
        })
    }

    /// Um compromisso: regra, versões, próximas ocorrências e histórico curto.
    ///
    /// # Errors
    /// Id inválido, compromisso inexistente ou falha do serviço.
    pub fn obter_compromisso(&self, a: &PedidoDeCompromisso) -> Resultado {
        self.executar("obter_compromisso", |p, hoje| {
            let id = texto(&a.id_ocorrencia, "id_ocorrencia", MAX_NOME)?;
            let meses = a.proximas.unwrap_or(12);
            if !(1..=60).contains(&meses) {
                return Err("proximas deve estar entre 1 e 60".to_owned());
            }
            let ocorrencia = IdOcorrencia::de_texto(&id);
            let (rec_id, desde) = match &ocorrencia {
                Some(o) => (o.recorrente.clone(), o.competencia),
                None => (id.clone(), Competencia::de_data(hoje)),
            };
            let rec = p
                .recorrente(&rec_id)
                .map_err(pt)?
                .ok_or("não encontrei esse compromisso (confira o id)")?;
            let proximas = p
                .proximas(&rec_id, desde, u16::try_from(meses).unwrap_or(60), hoje)
                .map_err(pt)?;
            let historico: Vec<Value> = p
                .historico(Some(&rec_id), 10)
                .map_err(pt)?
                .into_iter()
                .map(|h| {
                    json!({ "id": h.id, "quando": h.quando, "autor": h.autor, "resumo": h.resumo })
                })
                .collect();
            let esta = ocorrencia
                .as_ref()
                .and_then(|o| proximas.iter().find(|x| &x.id == o));
            Ok(json!({
                "compromisso": ok_serde(&rec)?,
                "ocorrencia": esta.map(ok_serde).transpose()?,
                "proximas_ocorrencias": ok_serde(&proximas)?,
                "historico": historico,
                "valores": "em centavos",
            }))
        })
    }

    /// O efeito de uma mudança, sem gravar nada.
    ///
    /// # Errors
    /// Argumento inválido ou falha do serviço.
    pub fn simular_mudanca(&self, a: &PedidoDeSimulacao) -> Resultado {
        self.executar("simular_mudanca", |p, hoje| {
            let alt = montar(p, &a.alteracao)?;
            let s = p.simular(&alt, hoje).map_err(pt)?;
            Ok(json!({ "simulacao": simulacao_json(&s), "grava": false, "valores": "em centavos" }))
        })
    }

    /// As propostas, mais novas primeiro.
    ///
    /// # Errors
    /// Argumento fora dos limites ou falha do serviço.
    pub fn listar_propostas(&self, a: &PedidoDePropostas) -> Resultado {
        self.executar("listar_propostas", |p, _| {
            let (pg, por) = pagina(a.pagina, a.por_pagina)?;
            let todas = p.propostas(a.so_pendentes.unwrap_or(true)).map_err(pt)?;
            let itens: Vec<_> = todas.iter().skip((pg - 1) * por).take(por).collect();
            Ok(json!({
                "total": todas.len(),
                "pagina": pg,
                "por_pagina": por,
                "itens": ok_serde(&itens)?,
                "valores": "em centavos",
            }))
        })
    }

    /// Guarda uma proposta pendente (nunca aplica).
    ///
    /// # Errors
    /// Argumento inválido, chave inválida ou falha do serviço.
    pub fn propor(&self, ferramenta: &str, chave: &str, e: &EntradaDeAlteracao) -> Resultado {
        self.executar(ferramenta, |p, hoje| {
            let chave = texto(chave, "chave", MAX_CHAVE)?;
            let alt = montar(p, e)?;
            let sim = p.simular(&alt, hoje).map_err(pt)?;
            let proposta = p.propor(alt.clone(), ORIGEM, &chave).map_err(pt)?;
            let mut v = json!({
                "mensagem": AVISO_DE_PROPOSTA,
                "proposta": {
                    "id": proposta.id,
                    "estado": ok_serde(&proposta.estado)?,
                    "chave": proposta.chave,
                    "expira_em": proposta.expira_em,
                },
                "simulacao": simulacao_json(&sim),
                "valores": "em centavos",
            });
            if proposta.alteracao != alt {
                v["aviso"] = json!(
                    "essa chave já tinha outra proposta: nada novo foi criado; use outra chave para uma proposta diferente"
                );
            }
            Ok(v)
        })
    }
}
