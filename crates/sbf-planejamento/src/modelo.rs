//! Modelo de compromisso e Cálculo: o molde que vários compromissos compartilham e o script
//! Luau que transforma os valores de cada compromisso no previsto de cada mês.
//!
//! - O banco guarda os modelos, as **versões imutáveis** do Cálculo (script + checksum), a
//!   ligação de cada compromisso a um modelo e os valores dos parâmetros, com vigência.
//! - O previsto de cada ocorrência é calculado **em lote por compromisso** (um pedido com
//!   todas as competências que a projeção vai usar) e guardado em cache pela chave
//!   (checksum do Cálculo, valores, competência, quantidade ajustada): nada é recalculado
//!   sem mudança.
//! - O script que falha nunca derruba a projeção: o previsto cai no último valor conhecido e
//!   a ocorrência leva um aviso com o motivo.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, params};
use sbf_regras::script::{
    ErroDoScript, ExecutorDeScript, ItemDeCalculo, Limites, PedidoDeCalculo, ResultadoDeCalculo,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::banco::{de_json, json, ler_competencia};
use crate::{
    Autor, Centavos, Competencia, Excecao, ExcecaoGravada, LigacaoDeModelo, Modelo,
    ParametroDeModelo, Resultado, TipoDeParametro, ValorDeParametro, ValorDoParametro,
};

/// O nome do modelo de fábrica.
pub const FABRICA_NOME_DO_MODELO: &str = "Diarista por dia da semana";
const FABRICA_ID_DO_MODELO: &str = "mod_fabrica_diarista";
const FABRICA_ID_DO_CALCULO: &str = "calc_fabrica_diarista_1";
const FABRICA_CALCULO: &str = include_str!("modelo/diarista.luau");

/// Quantas entradas o cache guarda antes de ser esvaziado.
const TETO_DO_CACHE: usize = 100_000;

/// O sha256 (hexadecimal) do script de um Cálculo.
pub(crate) fn checksum_do_calculo(fonte: &str) -> String {
    let digest = Sha256::digest(fonte.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------
// Banco
// ---------------------------------------------------------------------------------------

/// Semeia o modelo de fábrica (idempotente). Não grava histórico: é parte do banco, não
/// uma decisão do usuário.
pub(crate) fn semear_fabrica(c: &Connection) -> Resultado<()> {
    let parametros = vec![
        ParametroDeModelo {
            nome: "diaria".into(),
            tipo: TipoDeParametro::Centavos,
        },
        ParametroDeModelo {
            nome: "dia_da_semana".into(),
            tipo: TipoDeParametro::DiaDaSemana,
        },
    ];
    let quando = "1970-01-01T00:00:00Z";
    let autor = json(&Autor::Importacao)?;
    c.execute(
        "INSERT OR IGNORE INTO modelos (id, nome, parametros, quando, autor, desfeito, fabrica)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 1)",
        params![
            FABRICA_ID_DO_MODELO,
            FABRICA_NOME_DO_MODELO,
            json(&parametros)?,
            quando,
            autor
        ],
    )?;
    c.execute(
        "INSERT OR IGNORE INTO calculos (id, modelo_id, versao, fonte, checksum, quando, autor,
                                         desfeita)
         VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, 0)",
        params![
            FABRICA_ID_DO_CALCULO,
            FABRICA_ID_DO_MODELO,
            FABRICA_CALCULO,
            checksum_do_calculo(FABRICA_CALCULO),
            quando,
            autor
        ],
    )?;
    Ok(())
}

struct LinhaDeModelo {
    id: String,
    nome: String,
    parametros: String,
    fabrica: bool,
}

/// Os modelos vivos, cada um com a versão mais recente (e não desfeita) do Cálculo.
pub(crate) fn ler_modelos(c: &Connection) -> Resultado<Vec<Modelo>> {
    let mut stmt = c.prepare(
        "SELECT m.id, m.nome, m.parametros, m.fabrica,
                k.versao, k.checksum, k.fonte
         FROM modelos m
         JOIN calculos k ON k.modelo_id = m.id AND k.desfeita = 0
              AND k.versao = (SELECT MAX(versao) FROM calculos
                              WHERE modelo_id = m.id AND desfeita = 0)
         WHERE m.desfeito = 0
         ORDER BY m.nome, m.id",
    )?;
    let linhas = stmt
        .query_map([], |l| {
            Ok((
                LinhaDeModelo {
                    id: l.get(0)?,
                    nome: l.get(1)?,
                    parametros: l.get(2)?,
                    fabrica: l.get(3)?,
                },
                l.get::<_, u32>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    linhas
        .into_iter()
        .map(|(m, versao, checksum, calculo)| {
            Ok(Modelo {
                id: m.id,
                nome: m.nome,
                parametros: de_json(&m.parametros)?,
                versao_do_calculo: versao,
                checksum,
                calculo,
                de_fabrica: m.fabrica,
            })
        })
        .collect()
}

/// O modelo vivo `id`, se existir.
pub(crate) fn ler_modelo(c: &Connection, id: &str) -> Resultado<Option<Modelo>> {
    Ok(ler_modelos(c)?.into_iter().find(|m| m.id == id))
}

/// Uma ligação como o banco a guarda, com o Cálculo a que está presa.
#[derive(Debug, Clone)]
pub(crate) struct LigacaoLida {
    pub recorrente: String,
    pub modelo_id: String,
    pub modelo: String,
    pub versao: u32,
    pub fonte: String,
    pub checksum: String,
    pub parametros: Vec<ParametroDeModelo>,
    pub valores: Vec<ValorDeParametro>,
}

impl LigacaoLida {
    pub(crate) fn publica(&self) -> LigacaoDeModelo {
        LigacaoDeModelo {
            recorrente: self.recorrente.clone(),
            modelo_id: self.modelo_id.clone(),
            modelo: self.modelo.clone(),
            versao_do_calculo: self.versao,
            checksum: self.checksum.clone(),
            parametros: self.parametros.clone(),
            valores: self.valores.clone(),
        }
    }
}

/// As ligações vigentes (de um compromisso, ou de todos), com os valores dos parâmetros que
/// o modelo declara.
pub(crate) fn ler_ligacoes(
    c: &Connection,
    so_o: Option<&str>,
) -> Resultado<HashMap<String, LigacaoLida>> {
    let mut stmt = c.prepare(
        "SELECT l.recorrente_id, m.id, m.nome, m.parametros, k.id, k.versao, k.fonte, k.checksum
         FROM ligacoes_de_modelo l
         JOIN modelos m ON m.id = l.modelo_id
         JOIN calculos k ON k.id = l.calculo_id
         WHERE l.desligada = 0 AND (?1 IS NULL OR l.recorrente_id = ?1)",
    )?;
    let linhas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, u32>(5)?,
                l.get::<_, String>(6)?,
                l.get::<_, String>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut valores = ler_valores(c, so_o)?;
    let mut mapa = HashMap::new();
    for (recorrente, modelo_id, modelo, parametros, _calculo_id, versao, fonte, checksum) in linhas
    {
        let parametros: Vec<ParametroDeModelo> = de_json(&parametros)?;
        let declarados: BTreeSet<&str> = parametros.iter().map(|p| p.nome.as_str()).collect();
        let mut meus = valores.remove(&recorrente).unwrap_or_default();
        meus.retain(|v| declarados.contains(v.parametro.as_str()));
        meus.sort_by(|a, b| (&a.parametro, a.desde).cmp(&(&b.parametro, b.desde)));
        mapa.insert(
            recorrente.clone(),
            LigacaoLida {
                recorrente,
                modelo_id,
                modelo,
                versao,
                fonte,
                checksum,
                parametros,
                valores: meus,
            },
        );
    }
    Ok(mapa)
}

/// Os valores vivos (não desfeitos) dos parâmetros, por compromisso.
fn ler_valores(
    c: &Connection,
    so_o: Option<&str>,
) -> Resultado<HashMap<String, Vec<ValorDeParametro>>> {
    let mut stmt = c.prepare(
        "SELECT recorrente_id, parametro, desde, valor FROM valores_de_parametro
         WHERE desfeito = 0 AND (?1 IS NULL OR recorrente_id = ?1)",
    )?;
    let linhas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut mapa: HashMap<String, Vec<ValorDeParametro>> = HashMap::new();
    for (recorrente, parametro, desde, valor) in linhas {
        mapa.entry(recorrente).or_default().push(ValorDeParametro {
            parametro,
            desde: ler_competencia(&desde)?,
            valor: de_json(&valor)?,
        });
    }
    Ok(mapa)
}

// ---------------------------------------------------------------------------------------
// Validação dos valores
// ---------------------------------------------------------------------------------------

/// O maior valor de centavos aceito num parâmetro (1 bilhão de reais).
const MAX_CENTAVOS: i64 = 100_000_000_000;
const MAX_TEXTO_DO_PARAMETRO: usize = 200;

/// Confere o nome e o tipo do valor contra o que o modelo declara.
pub(crate) fn validar_valor(
    parametros: &[ParametroDeModelo],
    v: &ValorDeParametro,
) -> Result<(), String> {
    let Some(p) = parametros.iter().find(|p| p.nome == v.parametro) else {
        return Err(format!(
            "o modelo não tem o parâmetro `{}` (os dele: {})",
            v.parametro,
            parametros
                .iter()
                .map(|p| format!("`{}`", p.nome))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    match (p.tipo, &v.valor) {
        (TipoDeParametro::Centavos, ValorDoParametro::Inteiro(n)) => {
            if (0..=MAX_CENTAVOS).contains(n) {
                Ok(())
            } else {
                Err(format!(
                    "`{}` deve estar entre 0 e {MAX_CENTAVOS} centavos",
                    p.nome
                ))
            }
        }
        (TipoDeParametro::DiaDaSemana, ValorDoParametro::Inteiro(n)) => {
            if (1..=7).contains(n) {
                Ok(())
            } else {
                Err(format!(
                    "`{}` deve ser um dia da semana de 1 (segunda) a 7 (domingo)",
                    p.nome
                ))
            }
        }
        (TipoDeParametro::Inteiro, ValorDoParametro::Inteiro(n)) => {
            if n.abs() <= 1 << 40 {
                Ok(())
            } else {
                Err(format!("`{}` é grande demais", p.nome))
            }
        }
        (TipoDeParametro::Texto, ValorDoParametro::Texto(t)) => {
            if t.chars().count() <= MAX_TEXTO_DO_PARAMETRO {
                Ok(())
            } else {
                Err(format!(
                    "`{}` passa de {MAX_TEXTO_DO_PARAMETRO} caracteres",
                    p.nome
                ))
            }
        }
        (tipo, _) => Err(format!(
            "`{}` é do tipo {}: o valor veio de outro tipo",
            p.nome,
            nome_do_tipo(tipo)
        )),
    }
}

pub(crate) fn nome_do_tipo(t: TipoDeParametro) -> &'static str {
    match t {
        TipoDeParametro::Centavos => "centavos",
        TipoDeParametro::DiaDaSemana => "dia_da_semana",
        TipoDeParametro::Inteiro => "inteiro",
        TipoDeParametro::Texto => "texto",
    }
}

/// Um nome de parâmetro: minúsculas ASCII, dígitos e `_`, sem começar por dígito.
pub(crate) fn nome_de_parametro_valido(nome: &str) -> bool {
    let mut cs = nome.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && nome.len() <= 40
        && nome
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------------------------------
// O Cálculo de cada mês
// ---------------------------------------------------------------------------------------

/// O que o Cálculo diz de uma competência.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Calculado {
    /// O previsto. `None` quando falhou e não há valor conhecido antes (a projeção usa o da
    /// versão).
    pub valor: Option<Centavos>,
    pub quantidade: Option<i64>,
    pub explicacao: String,
    pub ajustada: bool,
    /// Por que não valeu.
    pub aviso: Option<String>,
    pub modelo: String,
}

/// O resultado cru de um mês, o que vai para o cache.
type Cru = Result<(i64, Option<i64>, String), String>;

type ChaveDoCache = (String, String, String, Option<i64>);

/// Quem executa o Cálculo e guarda o cache. Uma por serviço.
pub(crate) struct Calculadora {
    executor: Option<Arc<dyn ExecutorDeScript>>,
    cache: Mutex<HashMap<ChaveDoCache, Cru>>,
}

impl fmt::Debug for Calculadora {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Calculadora")
            .field("executor", &self.executor.is_some())
            .finish_non_exhaustive()
    }
}

impl Calculadora {
    pub(crate) fn nova() -> Calculadora {
        Calculadora {
            executor: None,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn com_executor(executor: Arc<dyn ExecutorDeScript>) -> Calculadora {
        Calculadora {
            executor: Some(executor),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Confere que o script compila e define `calcular`. Sem executor, não há como: passa.
    pub(crate) fn verificar(&self, fonte: &str) -> Result<(), String> {
        let Some(ex) = &self.executor else {
            return Ok(());
        };
        ex.verificar_calculo(fonte).map_err(|e| match e {
            ErroDoScript::SemCalcular => {
                "o script precisa definir a função `calcular(entrada, ajuda)`".to_owned()
            }
            outro => format!("o script não passou na verificação: {outro}"),
        })
    }

    /// Calcula, em lote, as competências de um compromisso ligado a um modelo. As que já
    /// estão no cache não vão ao executor. `quantidades` são os ajustes de um mês só.
    ///
    /// Competências anteriores ao primeiro valor de parâmetro não entram: o compromisso
    /// ainda não usava o modelo. Competência com parâmetro faltando, ou com o script
    /// falhando, vira um [`Calculado`] com aviso e o último valor conhecido.
    pub(crate) fn calcular(
        &self,
        ligacao: &LigacaoLida,
        competencias: impl IntoIterator<Item = Competencia>,
        quantidades: &HashMap<Competencia, i64>,
    ) -> HashMap<Competencia, Calculado> {
        let Some(primeiro) = ligacao.valores.iter().map(|v| v.desde).min() else {
            return HashMap::new();
        };
        let competencias: BTreeSet<Competencia> = competencias
            .into_iter()
            .filter(|c| *c >= primeiro)
            .collect();

        // 1. O item (ou o erro de parâmetro faltando) de cada mês.
        let mut itens: Vec<(Competencia, Result<ItemDeCalculo, String>)> = Vec::new();
        for c in &competencias {
            let item = parametros_vigentes(ligacao, *c).map(|parametros| ItemDeCalculo {
                competencia: c.to_string(),
                parametros,
                quantidade_ajustada: quantidades.get(c).copied(),
            });
            itens.push((*c, item));
        }

        // 2. O que o cache já sabe; o resto vai ao executor num pedido só.
        let mut crus: HashMap<Competencia, Cru> = HashMap::new();
        let mut faltam: Vec<(Competencia, ItemDeCalculo, ChaveDoCache)> = Vec::new();
        {
            let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            for (c, item) in &itens {
                match item {
                    Err(motivo) => {
                        crus.insert(*c, Err(motivo.clone()));
                    }
                    Ok(item) => {
                        let chave = (
                            ligacao.checksum.clone(),
                            item.competencia.clone(),
                            item.parametros.to_string(),
                            item.quantidade_ajustada,
                        );
                        match cache.get(&chave) {
                            Some(cru) => {
                                crus.insert(*c, cru.clone());
                            }
                            None => faltam.push((*c, item.clone(), chave)),
                        }
                    }
                }
            }
        }
        if !faltam.is_empty() {
            let respostas = self.executar(ligacao, faltam.iter().map(|(_, i, _)| i.clone()));
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if cache.len() + faltam.len() > TETO_DO_CACHE {
                cache.clear();
            }
            for ((c, _, chave), resposta) in faltam.into_iter().zip(respostas) {
                let (cru, guardar) = traduzir(resposta);
                if guardar {
                    cache.insert(chave, cru.clone());
                }
                crus.insert(c, cru);
            }
        }

        // 3. Em ordem cronológica: o que falha herda o último valor que deu certo.
        let mut ultimo: Option<i64> = None;
        let mut saida = HashMap::new();
        for c in competencias {
            let ajustada = quantidades.contains_key(&c);
            let calculado = match crus.remove(&c) {
                Some(Ok((valor, quantidade, explicacao))) => {
                    ultimo = Some(valor);
                    Calculado {
                        valor: Some(Centavos(valor)),
                        quantidade,
                        explicacao,
                        ajustada,
                        aviso: None,
                        modelo: ligacao.modelo.clone(),
                    }
                }
                Some(Err(motivo)) => Calculado {
                    valor: ultimo.map(Centavos),
                    quantidade: None,
                    explicacao: String::new(),
                    ajustada,
                    aviso: Some(format!(
                        "O cálculo não valeu em {c}: {motivo}. O previsto segue o último valor conhecido."
                    )),
                    modelo: ligacao.modelo.clone(),
                },
                None => continue,
            };
            saida.insert(c, calculado);
        }
        saida
    }

    fn executar(
        &self,
        ligacao: &LigacaoLida,
        itens: impl Iterator<Item = ItemDeCalculo>,
    ) -> Vec<ResultadoDeCalculo> {
        let itens: Vec<ItemDeCalculo> = itens.collect();
        let n = itens.len();
        let Some(ex) = &self.executor else {
            return vec![
                Err(ErroDoScript::Processo(
                    "não há executor de cálculo neste ponto do app".to_owned(),
                ));
                n
            ];
        };
        let pedido = PedidoDeCalculo {
            fonte: ligacao.fonte.clone(),
            checksum: ligacao.checksum.clone(),
            itens,
            limites: Limites::default(),
        };
        match ex.calcular(&pedido) {
            Ok(r) if r.len() == n => r,
            Ok(_) => vec![
                Err(ErroDoScript::Processo(
                    "o executor devolveu o número errado de meses".to_owned(),
                ));
                n
            ],
            Err(e) => vec![Err(e); n],
        }
    }
}

/// O resultado cru de um mês, e se vale guardá-lo no cache (erro de processo é passageiro).
fn traduzir(r: ResultadoDeCalculo) -> (Cru, bool) {
    match r {
        Ok(x) => (Ok((x.valor_centavos, x.quantidade, x.explicacao)), true),
        Err(e) => {
            let passageiro = matches!(e, ErroDoScript::Processo(_));
            (Err(motivo_do_erro(&e)), !passageiro)
        }
    }
}

fn motivo_do_erro(e: &ErroDoScript) -> String {
    match e {
        ErroDoScript::Execucao(m) | ErroDoScript::Retorno(m) | ErroDoScript::Sintaxe(m) => {
            format!("o script falhou ({m})")
        }
        outro => format!("o script falhou ({outro})"),
    }
}

/// `{nome: valor}` dos parâmetros do modelo vigentes na competência.
fn parametros_vigentes(ligacao: &LigacaoLida, c: Competencia) -> Result<Value, String> {
    let mut mapa = Map::new();
    for p in &ligacao.parametros {
        let vigente = ligacao
            .valores
            .iter()
            .filter(|v| v.parametro == p.nome && v.desde <= c)
            .max_by_key(|v| v.desde);
        let Some(v) = vigente else {
            return Err(format!("falta o valor do parâmetro `{}`", p.nome));
        };
        mapa.insert(
            p.nome.clone(),
            match &v.valor {
                ValorDoParametro::Inteiro(n) => json!(n),
                ValorDoParametro::Texto(t) => json!(t),
            },
        );
    }
    Ok(Value::Object(mapa))
}

/// As quantidades digitadas só para um mês (as exceções vivas, a última de cada mês vale).
pub(crate) fn quantidades_ajustadas(excecoes: &[ExcecaoGravada]) -> HashMap<Competencia, i64> {
    let mut mapa = HashMap::new();
    for e in excecoes.iter().filter(|e| !e.desfeita) {
        if let Excecao::QuantidadeSoDesteMes { quantidade } = e.excecao {
            mapa.insert(e.ocorrencia.competencia, quantidade);
        }
    }
    mapa
}

/// O Cálculo de um compromisso para as competências dadas. Sem ligação, nada.
pub(crate) fn calculados_de(
    calculadora: &Calculadora,
    ligacao: Option<&LigacaoLida>,
    excecoes: &[ExcecaoGravada],
    competencias: impl IntoIterator<Item = Competencia>,
) -> HashMap<Competencia, Calculado> {
    let Some(ligacao) = ligacao else {
        return HashMap::new();
    };
    calculadora.calcular(ligacao, competencias, &quantidades_ajustadas(excecoes))
}

/// Já existe um modelo vivo com este nome?
pub(crate) fn existe_modelo_vivo(c: &Connection, nome: &str) -> Resultado<bool> {
    Ok(c.query_row(
        "SELECT 1 FROM modelos WHERE nome = ?1 AND desfeito = 0",
        params![nome],
        |_| Ok(()),
    )
    .optional()?
    .is_some())
}
