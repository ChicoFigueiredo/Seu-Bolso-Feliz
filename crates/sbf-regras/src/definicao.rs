//! O formato declarativo de uma versão de regra (JSON, spec 13).
//!
//! Uma [`Definicao`] diz como **detectar** um tipo de documento ou registro, quais campos
//! **extrair** (regex + formato), o que é **obrigatório**, qual leitor nativo ou script
//! complementa e quais **propostas** sair. Compilada uma vez ([`Definicao::compilar`]), é
//! avaliada pelo [`crate::motor::Motor`] sem recompilar Rust.
//!
//! ## Especificidade
//!
//! [`Definicao::especificidade`] é o número de condições em `todos` + número de condições
//! em `nenhum` + 1 se `algum` não for vazio. Quanto mais restrições, mais específica a
//! regra; é o desempate da precedência depois da `prioridade`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::{
    Campo, Canal, DeOnde, Entrada, ErroDasRegras, Proposta, TipoDeEntrada, TipoDoDocumento,
    VERSAO_DO_ESQUEMA, Valor,
};

/// Teto de tamanho de um regex compilado (evita regex patológico vindo de uma regra).
const LIMITE_DO_REGEX: usize = 1 << 20;

fn esquema_padrao() -> u32 {
    VERSAO_DO_ESQUEMA
}

fn aplica_padrao() -> Vec<TipoDeEntrada> {
    vec![TipoDeEntrada::Documento]
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn grupo_padrao() -> usize {
    1
}

/// Uma versão de regra, no formato declarativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definicao {
    #[serde(default = "esquema_padrao")]
    pub versao_do_esquema: u32,
    pub tipo_documental: String,
    #[serde(default = "aplica_padrao")]
    pub aplica_a: Vec<TipoDeEntrada>,
    #[serde(default)]
    pub prioridade: i32,
    pub detectar: Detectar,
    #[serde(default)]
    pub extrair: BTreeMap<String, Extracao>,
    #[serde(default)]
    pub obrigatorios: Vec<String>,
    /// Um leitor nativo do sbf (`contracheque`) que quem chama roda e cujos campos voltam
    /// ao motor por [`crate::motor::Motor::completar`].
    #[serde(default)]
    pub leitor: Option<String>,
    /// Nomes dos parâmetros versionados que o script recebe.
    #[serde(default)]
    pub parametros: Vec<String>,
    /// O script recebe o texto do documento (padrão: não).
    #[serde(default)]
    pub script_le_texto: bool,
    /// O texto do documento vem de OCR (imagem): antes de detectar e extrair, o motor o
    /// passa por [`normalizar_ocr`] (espaços juntos, `O`/`l` consertados dentro de números,
    /// `RS`/`R§` lidos como `R$`). Falso por padrão, e **não entra no JSON quando é falso**,
    /// para não mudar o checksum das versões que já existem.
    #[serde(default, skip_serializing_if = "is_false")]
    pub texto_de_ocr: bool,
    #[serde(default)]
    pub propor: Vec<ModeloDeProposta>,
}

impl Definicao {
    /// Uma definição mínima: detecta pelo `contem` e não extrai nada.
    pub fn simples(tipo_documental: &str, contem: &str) -> Self {
        Self {
            versao_do_esquema: VERSAO_DO_ESQUEMA,
            tipo_documental: tipo_documental.to_owned(),
            aplica_a: aplica_padrao(),
            prioridade: 0,
            detectar: Detectar {
                todos: vec![Condicao::Contem(contem.to_owned())],
                algum: Vec::new(),
                nenhum: Vec::new(),
            },
            extrair: BTreeMap::new(),
            obrigatorios: Vec::new(),
            leitor: None,
            parametros: Vec::new(),
            script_le_texto: false,
            texto_de_ocr: false,
            propor: Vec::new(),
        }
    }

    /// Número de condições em `todos` e `nenhum`, mais 1 se `algum` não for vazio.
    pub fn especificidade(&self) -> usize {
        self.detectar.todos.len()
            + self.detectar.nenhum.len()
            + usize::from(!self.detectar.algum.is_empty())
    }

    /// Confere a definição: esquema, tipo, nomes e regexes.
    ///
    /// # Errors
    /// [`ErroDasRegras::Definicao`] com o que está errado.
    pub fn validar(&self) -> Result<(), ErroDasRegras> {
        self.compilar().map(|_| ())
    }

    /// Valida e compila os regexes.
    ///
    /// # Errors
    /// [`ErroDasRegras::Definicao`].
    pub fn compilar(&self) -> Result<Compilada, ErroDasRegras> {
        let erro = |m: String| ErroDasRegras::Definicao(m);
        if self.versao_do_esquema != VERSAO_DO_ESQUEMA {
            return Err(erro(format!(
                "versao_do_esquema {} não suportada (esperado {VERSAO_DO_ESQUEMA})",
                self.versao_do_esquema
            )));
        }
        if self.tipo_documental.trim().is_empty() {
            return Err(erro("tipo_documental vazio".to_owned()));
        }
        if self.aplica_a.is_empty() {
            return Err(erro("aplica_a vazio".to_owned()));
        }
        if self.detectar.todos.is_empty() && self.detectar.algum.is_empty() {
            return Err(erro(
                "detectar precisa de ao menos uma condição em `todos` ou `algum`".to_owned(),
            ));
        }
        for nome in self
            .extrair
            .keys()
            .chain(self.obrigatorios.iter())
            .chain(self.parametros.iter())
        {
            if !nome_valido(nome) {
                return Err(erro(format!("nome inválido `{nome}` (use [a-z0-9_]+)")));
            }
        }
        for proposta in &self.propor {
            if let ModeloDeProposta::VincularOcorrencia { compromisso, .. } = proposta
                && normalizar(compromisso).is_empty()
            {
                return Err(erro(
                    "propor `vincular_ocorrencia`: `compromisso` vazio".to_owned(),
                ));
            }
        }
        let todos = compilar_condicoes(&self.detectar.todos)?;
        let algum = compilar_condicoes(&self.detectar.algum)?;
        let nenhum = compilar_condicoes(&self.detectar.nenhum)?;
        let mut extracoes = Vec::new();
        for (nome, e) in &self.extrair {
            let regex = compilar_regex(&e.regex)
                .map_err(|m| erro(format!("extrair `{nome}`: regex inválido: {m}")))?;
            if e.grupo >= regex.captures_len() {
                return Err(erro(format!(
                    "extrair `{nome}`: o regex não tem o grupo {}",
                    e.grupo
                )));
            }
            extracoes.push(ExtracaoCompilada {
                nome: nome.clone(),
                regex,
                grupo: e.grupo,
                formato: e.formato,
                fonte: e.fonte,
            });
        }
        Ok(Compilada {
            todos,
            algum,
            nenhum,
            extracoes,
        })
    }
}

fn nome_valido(nome: &str) -> bool {
    !nome.is_empty()
        && nome
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn compilar_regex(padrao: &str) -> Result<Regex, String> {
    RegexBuilder::new(padrao)
        .size_limit(LIMITE_DO_REGEX)
        .dfa_size_limit(LIMITE_DO_REGEX)
        .build()
        .map_err(|e| e.to_string())
}

fn compilar_condicoes(condicoes: &[Condicao]) -> Result<Vec<CondicaoCompilada>, ErroDasRegras> {
    condicoes
        .iter()
        .map(|c| {
            let regex = |p: &str| {
                compilar_regex(p)
                    .map_err(|m| ErroDasRegras::Definicao(format!("regex inválido em `{p}`: {m}")))
            };
            Ok(match c {
                Condicao::Contem(t) => CondicaoCompilada::Contem(normalizar(t)),
                Condicao::Regex(p) => CondicaoCompilada::Regex(regex(p)?),
                Condicao::Mime(m) => {
                    CondicaoCompilada::Mime(m.iter().map(|x| x.to_ascii_lowercase()).collect())
                }
                Condicao::Canal(c) => CondicaoCompilada::Canal(c.clone()),
                Condicao::Descricao(p) => CondicaoCompilada::Descricao(regex(p)?),
                Condicao::Nome(p) => CondicaoCompilada::Nome(regex(p)?),
                Condicao::ContraparteDoc(docs) => {
                    CondicaoCompilada::ContraparteDoc(documentos_validos(docs)?)
                }
                Condicao::Contraparte(p) => CondicaoCompilada::Contraparte(
                    RegexBuilder::new(p)
                        .case_insensitive(true)
                        .size_limit(LIMITE_DO_REGEX)
                        .dfa_size_limit(LIMITE_DO_REGEX)
                        .build()
                        .map_err(|m| {
                            ErroDasRegras::Definicao(format!("regex inválido em `{p}`: {m}"))
                        })?,
                ),
            })
        })
        .collect()
}

/// Os dígitos de um documento (CPF/CNPJ), sem a pontuação.
pub fn so_os_digitos(documento: &str) -> String {
    documento.chars().filter(char::is_ascii_digit).collect()
}

/// Confere a lista de `contraparte_doc`: ao menos um, cada um com 11 (CPF) ou 14 (CNPJ)
/// dígitos. Devolve só os dígitos.
fn documentos_validos(docs: &[String]) -> Result<Vec<String>, ErroDasRegras> {
    if docs.is_empty() {
        return Err(ErroDasRegras::Definicao(
            "contraparte_doc precisa de ao menos um documento".to_owned(),
        ));
    }
    docs.iter()
        .map(|d| {
            let digitos = so_os_digitos(d);
            if matches!(digitos.len(), 11 | 14) {
                Ok(digitos)
            } else {
                // Sem repetir o documento na mensagem: é dado pessoal.
                Err(ErroDasRegras::Definicao(
                    "contraparte_doc: cada documento tem 11 (CPF) ou 14 (CNPJ) dígitos".to_owned(),
                ))
            }
        })
        .collect()
}

/// As condições de detecção. Casa quando todas de `todos`, ao menos uma de `algum` (se não
/// vazio) e nenhuma de `nenhum` são verdadeiras.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detectar {
    #[serde(default)]
    pub todos: Vec<Condicao>,
    #[serde(default)]
    pub algum: Vec<Condicao>,
    #[serde(default)]
    pub nenhum: Vec<Condicao>,
}

/// Uma condição de detecção.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condicao {
    /// O texto normalizado (minúsculas, sem acento, espaços colapsados) contém este trecho
    /// (também normalizado).
    Contem(String),
    /// Regex sobre o texto original.
    Regex(String),
    /// O tipo real do conteúdo é um destes (`application/pdf`, `image/*`).
    Mime(Vec<String>),
    /// O canal da origem é um destes.
    Canal(Vec<Canal>),
    /// Regex sobre `dicas.descricao`.
    Descricao(String),
    /// Regex sobre o nome do arquivo.
    Nome(String),
    /// O CPF/CNPJ da contraparte do registro (`dicas.contraparte_doc`) é igual a algum destes.
    /// Só os dígitos contam: `000.000.000-00` e `00000000000` são o mesmo documento.
    ContraparteDoc(Vec<String>),
    /// Regex sobre o nome da contraparte (`dicas.contraparte`) **normalizado** (minúsculas,
    /// sem acento, espaços juntos), sem diferenciar maiúsculas do padrão.
    Contraparte(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formato {
    Texto,
    /// Dinheiro em centavos inteiros (ver [`parse_centavos`]).
    Centavos,
    /// `AAAA-MM-DD`.
    Data,
    /// `AAAA-MM`.
    Competencia,
    Inteiro,
    /// Texto em minúsculas (para constantes como `pix`).
    Minusculas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FonteDaExtracao {
    /// O texto do documento.
    #[default]
    Texto,
    /// `dicas.descricao`.
    Descricao,
    /// `dicas.contraparte`: o nome do favorecido do registro.
    Contraparte,
    /// `dicas.contraparte_doc`: o CPF/CNPJ do favorecido. Só vale para o motor; o documento
    /// nunca é gravado na execução nem na proposta.
    ContraparteDoc,
}

/// Como extrair um campo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extracao {
    pub regex: String,
    #[serde(default = "grupo_padrao")]
    pub grupo: usize,
    pub formato: Formato,
    #[serde(default)]
    pub fonte: FonteDaExtracao,
}

/// O que propor com os campos extraídos. Os campos usados são `valor_centavos`, `data`
/// (ou `vencimento`, na fatura), `conta` e `cartao`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeloDeProposta {
    ProvarRegistro {
        janela_dias: u32,
    },
    ConciliarOcorrencia {
        janela_dias: u32,
        #[serde(default)]
        compromisso: Option<String>,
    },
    PagarFatura,
    /// Anexa o documento à ocorrência do mês do compromisso (o campo `data`, ou `vencimento`);
    /// não pede valor. Os dois campos são obrigatórios na definição.
    VincularOcorrencia {
        /// Um trecho do nome do compromisso, comparado sem acento nem maiúsculas.
        compromisso: String,
        tipo_documento: TipoDoDocumento,
        janela_dias: u32,
    },
}

impl ModeloDeProposta {
    /// Monta a proposta ou diz o que falta.
    ///
    /// # Errors
    /// O motivo, em texto, de a proposta não sair.
    pub fn gerar(&self, campos: &BTreeMap<String, Campo>) -> Result<Proposta, String> {
        let centavos = |nome: &str| match campos.get(nome).map(|c| &c.valor) {
            Some(Valor::Centavos(c)) => Ok(*c),
            _ => Err(format!("falta o campo `{nome}` (centavos)")),
        };
        let data = |nome: &str| match campos.get(nome).map(|c| &c.valor) {
            Some(Valor::Data(d)) => Ok(d.clone()),
            _ => Err(format!("falta o campo `{nome}` (data)")),
        };
        let texto = |nome: &str| match campos.get(nome).map(|c| &c.valor) {
            Some(Valor::Texto(t)) => Some(t.clone()),
            _ => None,
        };
        match self {
            Self::ProvarRegistro { janela_dias } => Ok(Proposta::ProvarRegistro {
                valor_centavos: centavos("valor_centavos")
                    .map_err(|m| format!("proposta provar_registro não gerada: {m}"))?,
                data: data("data")
                    .map_err(|m| format!("proposta provar_registro não gerada: {m}"))?,
                janela_dias: *janela_dias,
                conta: texto("conta"),
            }),
            Self::ConciliarOcorrencia {
                janela_dias,
                compromisso,
            } => Ok(Proposta::ConciliarOcorrencia {
                valor_centavos: centavos("valor_centavos")
                    .map_err(|m| format!("proposta conciliar_ocorrencia não gerada: {m}"))?,
                data: data("data")
                    .map_err(|m| format!("proposta conciliar_ocorrencia não gerada: {m}"))?,
                janela_dias: *janela_dias,
                compromisso: compromisso.clone(),
            }),
            Self::VincularOcorrencia {
                compromisso,
                tipo_documento,
                janela_dias,
            } => Ok(Proposta::VincularOcorrencia {
                data: data("data")
                    .or_else(|_| data("vencimento"))
                    .map_err(|_| {
                        "proposta vincular_ocorrencia não gerada: falta o campo `data` (ou `vencimento`)"
                            .to_owned()
                    })?,
                janela_dias: *janela_dias,
                compromisso: compromisso.trim().to_owned(),
                tipo_documento: *tipo_documento,
            }),
            Self::PagarFatura => Ok(Proposta::PagarFatura {
                valor_centavos: centavos("valor_centavos")
                    .map_err(|m| format!("proposta pagar_fatura não gerada: {m}"))?,
                vencimento: data("vencimento")
                    .map_err(|m| format!("proposta pagar_fatura não gerada: {m}"))?,
                cartao: texto("cartao"),
            }),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Forma compilada
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum CondicaoCompilada {
    Contem(String),
    Regex(Regex),
    Mime(Vec<String>),
    Canal(Vec<Canal>),
    Descricao(Regex),
    Nome(Regex),
    /// Só dígitos.
    ContraparteDoc(Vec<String>),
    Contraparte(Regex),
}

impl CondicaoCompilada {
    fn casa(&self, entrada: &Entrada, normalizado: &str) -> bool {
        match self {
            Self::Contem(t) => !normalizado.is_empty() && normalizado.contains(t.as_str()),
            Self::Regex(r) => entrada.texto.as_deref().is_some_and(|t| r.is_match(t)),
            Self::Mime(lista) => entrada.mime.as_deref().is_some_and(|m| {
                let m = m.to_ascii_lowercase();
                lista.iter().any(|p| match p.strip_suffix("/*") {
                    Some(prefixo) => m.starts_with(&format!("{prefixo}/")),
                    None => *p == m,
                })
            }),
            Self::Canal(lista) => lista.contains(&entrada.origem.canal),
            Self::Descricao(r) => entrada
                .dicas
                .descricao
                .as_deref()
                .is_some_and(|d| r.is_match(d)),
            Self::Nome(r) => entrada.nome.as_deref().is_some_and(|n| r.is_match(n)),
            Self::ContraparteDoc(docs) => entrada
                .dicas
                .contraparte_doc
                .as_deref()
                .map(so_os_digitos)
                .is_some_and(|d| !d.is_empty() && docs.contains(&d)),
            Self::Contraparte(r) => entrada
                .dicas
                .contraparte
                .as_deref()
                .map(normalizar)
                .is_some_and(|n| r.is_match(&n)),
        }
    }
}

#[derive(Debug, Clone)]
struct ExtracaoCompilada {
    nome: String,
    regex: Regex,
    grupo: usize,
    formato: Formato,
    fonte: FonteDaExtracao,
}

/// Uma [`Definicao`] com os regexes compilados.
#[derive(Debug, Clone)]
pub struct Compilada {
    todos: Vec<CondicaoCompilada>,
    algum: Vec<CondicaoCompilada>,
    nenhum: Vec<CondicaoCompilada>,
    extracoes: Vec<ExtracaoCompilada>,
}

impl Compilada {
    /// A entrada casa com a detecção? `normalizado` é o texto da entrada já passado por
    /// [`normalizar`] (o motor calcula uma vez por entrada).
    pub fn detecta(&self, entrada: &Entrada, normalizado: &str) -> bool {
        self.todos.iter().all(|c| c.casa(entrada, normalizado))
            && (self.algum.is_empty() || self.algum.iter().any(|c| c.casa(entrada, normalizado)))
            && !self.nenhum.iter().any(|c| c.casa(entrada, normalizado))
    }

    /// Extrai os campos declarados. O que não casa fica de fora; o que casa mas não tem o
    /// formato esperado fica de fora com um motivo. Todo campo leva a [`DeOnde`].
    pub fn extrair(&self, entrada: &Entrada) -> (BTreeMap<String, Campo>, Vec<String>) {
        let mut campos = BTreeMap::new();
        let mut motivos = Vec::new();
        for e in &self.extracoes {
            let (texto, dica) = match e.fonte {
                FonteDaExtracao::Texto => (entrada.texto.as_deref(), None),
                FonteDaExtracao::Descricao => {
                    (entrada.dicas.descricao.as_deref(), Some("descricao"))
                }
                FonteDaExtracao::Contraparte => {
                    (entrada.dicas.contraparte.as_deref(), Some("contraparte"))
                }
                FonteDaExtracao::ContraparteDoc => (
                    entrada.dicas.contraparte_doc.as_deref(),
                    Some("contraparte_doc"),
                ),
            };
            let Some(texto) = texto else { continue };
            let Some(caps) = e.regex.captures(texto) else {
                continue;
            };
            let Some(m) = caps.get(e.grupo) else { continue };
            // Trecho sem os espaços das pontas, para a posição apontar para o valor.
            let bruto = m.as_str();
            let inicio = m.start() + (bruto.len() - bruto.trim_start().len());
            let fim = m.end() - (bruto.len() - bruto.trim_end().len());
            if inicio >= fim {
                continue;
            }
            let recorte = &texto[inicio..fim];
            match e.formato.aplicar_em(recorte, entrada.momento.as_deref()) {
                Some(valor) => {
                    let de_onde = match dica {
                        Some(nome) => DeOnde::Dica {
                            nome: nome.to_owned(),
                        },
                        None => DeOnde::Trecho { inicio, fim },
                    };
                    campos.insert(e.nome.clone(), Campo { valor, de_onde });
                }
                None => motivos.push(e.formato.motivo_da_falha(
                    &e.nome,
                    recorte,
                    entrada.momento.as_deref(),
                )),
            }
        }
        (campos, motivos)
    }
}

impl Formato {
    /// Converte o trecho bruto; `None` se não tem o formato. Sem o `momento` do item (veja
    /// [`Formato::aplicar_em`]), um mês sem ano não vira data nem competência.
    pub fn aplicar(self, bruto: &str) -> Option<Valor> {
        self.aplicar_em(bruto, None)
    }

    /// Como [`Formato::aplicar`], completando o ano que falta (`25 de Novembro`, `Novembro`)
    /// pelo `momento` do item (ISO 8601; só a data conta): vale o ano que deixa o mês, ou a
    /// data, mais perto do momento.
    pub fn aplicar_em(self, bruto: &str, momento: Option<&str>) -> Option<Valor> {
        match self {
            Self::Texto => {
                let t = bruto.trim();
                (!t.is_empty()).then(|| Valor::Texto(t.to_owned()))
            }
            Self::Minusculas => {
                let t = bruto.trim();
                (!t.is_empty()).then(|| Valor::Texto(t.to_lowercase()))
            }
            Self::Centavos => parse_centavos(bruto).map(Valor::Centavos),
            Self::Data => parse_data(bruto)
                .or_else(|| data_por_extenso(bruto, momento))
                .map(Valor::Data),
            Self::Competencia => parse_competencia(bruto)
                .or_else(|| competencia_por_extenso(bruto, momento))
                .map(Valor::Competencia),
            Self::Inteiro => parse_inteiro(bruto).map(Valor::Inteiro),
        }
    }

    /// Por que o trecho achado pelo regex não virou valor (para o motivo da leitura e para o
    /// diagnóstico de Rita). O trecho é cortado.
    pub fn motivo_da_falha(self, campo: &str, bruto: &str, momento: Option<&str>) -> String {
        let trecho: String = bruto.trim().chars().take(40).collect();
        if matches!(self, Self::Data | Self::Competencia)
            && let Some(m) = so_o_mes(bruto)
            && m.ano.is_none()
        {
            let porque = if momento.and_then(data_do_momento).is_none() {
                "e o item não tem `momento` (data da mensagem ou do arquivo) para completar o ano"
            } else {
                "e o `momento` do item não ajuda"
            };
            return format!(
                "campo `{campo}`: «{trecho}» está sem ano {porque}; capture um trecho com o ano (ex.: `10/2026`)"
            );
        }
        if self == Self::Data && so_o_mes(bruto).is_some_and(|m| m.dia.is_none()) {
            return format!(
                "campo `{campo}`: «{trecho}» não tem o dia; o formato `data` pede dia, mês e ano"
            );
        }
        format!("campo `{campo}` achado mas fora do formato {self:?} (trecho «{trecho}»)")
    }
}

// ---------------------------------------------------------------------------------------
// Texto e parsers
// ---------------------------------------------------------------------------------------

/// Minúsculas, sem acento, só letras, dígitos e espaço simples (mesmo espírito de
/// `sbf_planilha::texto::normalizar`; este crate não depende da planilha).
pub fn normalizar(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for c in texto.chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            outro => outro.to_ascii_lowercase(),
        };
        if c.is_ascii_alphanumeric() {
            saida.push(c);
        } else if !saida.ends_with(' ') && !saida.is_empty() {
            saida.push(' ');
        }
    }
    saida.trim_end().to_owned()
}

/// O texto de um OCR com o ruído mais comum desfeito, para a regra que pede
/// `"texto_de_ocr": true`:
///
/// - espaços e tabs seguidos viram um espaço, e o fim de cada linha é aparado (as quebras
///   de linha ficam, porque os regexes das regras as usam);
/// - `RS` e `R§` viram `R$` quando vêm antes de um valor com vírgula e dois decimais
///   (`RS 1.234,56`; o `RS` de Rio Grande do Sul, antes de um CEP, fica);
/// - dentro de um **número** (um token só de dígitos, `.`, `,`, `/`, `:`, `-`, `O` e `l`,
///   com ao menos um dígito de verdade), `O` vira `0` e `l` vira `1`. Palavra, ou token com
///   outras letras (`Pol1`, `BOL2`), não é tocada.
///
/// Idempotente. Só `O` e `l` são trocados; o resto do texto passa como veio.
pub fn normalizar_ocr(texto: &str) -> String {
    static ESPACOS: LazyLock<Regex> = LazyLock::new(|| regex_fixo(r"[ \t\u{00A0}]+"));
    static FIM_DE_LINHA: LazyLock<Regex> = LazyLock::new(|| regex_fixo(r"(?m) +(\r?\n|$)"));
    static MOEDA: LazyLock<Regex> =
        LazyLock::new(|| regex_fixo(r"\bR[S§](\s*)([0-9Ol.]+,[0-9Ol]{2})\b"));
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| regex_fixo(r"[\p{L}\p{N}.,/:\-]+"));
    let juntos = ESPACOS.replace_all(texto, " ");
    let aparado = FIM_DE_LINHA.replace_all(&juntos, "$1");
    let moeda = MOEDA.replace_all(&aparado, "R$$${1}${2}");
    TOKEN
        .replace_all(&moeda, |c: &regex::Captures<'_>| {
            let t = &c[0];
            let so_de_numero = t.chars().all(|ch| {
                ch.is_ascii_digit() || matches!(ch, 'O' | 'l' | '.' | ',' | '/' | ':' | '-')
            });
            if so_de_numero && t.chars().any(|ch| ch.is_ascii_digit()) {
                t.replace('O', "0").replace('l', "1")
            } else {
                t.to_owned()
            }
        })
        .into_owned()
}

fn regex_fixo(padrao: &str) -> Regex {
    // Os padrões são constantes deste arquivo e compilam (os testes de `normalizar_ocr`
    // exercitam os quatro). Se um dia não compilar, o regex que não casa nada deixa o
    // texto como veio.
    RegexBuilder::new(padrao)
        .build()
        .unwrap_or_else(|_| Regex::new(r"[^\s\S]").unwrap_or_else(|_| unreachable!()))
}

/// Dinheiro em centavos. Padrão brasileiro primeiro:
///
/// - com vírgula, a vírgula é o decimal e o ponto é milhar: `1.234,56`, `R$ 1.234,56`,
///   `-12,30`, `(12,30)` (negativo), `12,5` (= 12,50);
/// - sem vírgula e com um único ponto seguido de 1 ou 2 dígitos, o ponto é decimal:
///   `1234.56`;
/// - sem vírgula, ponto seguido de 3 dígitos é milhar: `1.234` = R$ 1.234,00;
/// - só dígitos são reais inteiros: `1234` = R$ 1.234,00;
/// - mais de 2 casas decimais, lixo ou vazio: `None`.
pub fn parse_centavos(bruto: &str) -> Option<i64> {
    let mut s = bruto.trim();
    let mut negativo = false;
    if s.starts_with('(') && s.ends_with(')') && s.len() >= 2 {
        negativo = true;
        s = s[1..s.len() - 1].trim();
    }
    for _ in 0..2 {
        if let Some(r) = s.strip_prefix('-').or_else(|| s.strip_prefix('\u{2212}')) {
            negativo = !negativo;
            s = r.trim_start();
        }
        if let Some(r) = s.strip_prefix("R$") {
            s = r.trim_start();
        }
    }
    if s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b',')
    {
        return None;
    }
    let (inteira, fracao) = if let Some(i) = s.rfind(',') {
        if s[..i].contains(',') {
            return None;
        }
        (s[..i].replace('.', ""), &s[i + 1..])
    } else if let Some(i) = s.rfind('.') {
        let frac = &s[i + 1..];
        if s.matches('.').count() == 1 && (1..=2).contains(&frac.len()) {
            (s[..i].to_owned(), frac)
        } else {
            (s.replace('.', ""), "")
        }
    } else {
        (s.to_owned(), "")
    };
    if inteira.is_empty() || fracao.len() > 2 {
        return None;
    }
    if s.contains(',') && fracao.is_empty() {
        return None;
    }
    let reais: i64 = inteira.parse().ok()?;
    let mut centavos: i64 = if fracao.is_empty() {
        0
    } else {
        fracao.parse().ok()?
    };
    if fracao.len() == 1 {
        centavos *= 10;
    }
    let total = reais.checked_mul(100)?.checked_add(centavos)?;
    Some(if negativo { -total } else { total })
}

fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn so_digitos(s: &str, tamanho: usize) -> Option<u32> {
    (s.len() == tamanho && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// `06/10/2026`, `06-10-2026` ou `2026-10-06` viram `2026-10-06`; data impossível, `None`.
pub fn parse_data(bruto: &str) -> Option<String> {
    let s = bruto.trim();
    let (d, m, a) = if let Some((a, resto)) = s.split_once('-')
        && a.len() == 4
    {
        let (m, d) = resto.split_once('-')?;
        (d, m, a)
    } else if s.contains('/') || s.matches('-').count() == 2 {
        let sep = if s.contains('/') { '/' } else { '-' };
        let mut it = s.split(sep);
        let (d, m, a) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() {
            return None;
        }
        (d, m, a)
    } else {
        // `25 de novembro de 2026`, `November 20, 2026`: só com o ano; sem ano, `aplicar_em`.
        return data_por_extenso(s, None);
    };
    let dia = so_digitos(d, 2)?;
    let mes = so_digitos(m, 2)?;
    let ano = so_digitos(a, 4)?;
    let ano_i = i32::try_from(ano).ok()?;
    if !(1..=12).contains(&mes) || dia == 0 || dia > dias_no_mes(ano_i, mes) {
        return None;
    }
    Some(format!("{ano:04}-{mes:02}-{dia:02}"))
}

/// `10/2026` ou `2026-10` viram `2026-10`.
pub fn parse_competencia(bruto: &str) -> Option<String> {
    let s = bruto.trim();
    let (m, a) = if let Some((a, m)) = s.split_once('-')
        && a.len() == 4
    {
        (m, a)
    } else {
        s.split_once('/')?
    };
    let mes = so_digitos(m, 2)?;
    let ano = so_digitos(a, 4)?;
    (1..=12)
        .contains(&mes)
        .then(|| format!("{ano:04}-{mes:02}"))
}

const MESES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "marco",
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

const MESES_EM_INGLES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// O número do mês (1 a 12) de `janeiro`, `Março`, `mar`, `November`, `Sept`... (sem
/// diferenciar maiúsculas nem acentos). A abreviação é a de três letras (mais `sept`). Os
/// dois idiomas valem juntos: nenhum trio de letras é um mês num idioma e outro no outro.
fn mes_por_nome(palavra: &str) -> Option<u32> {
    let p = normalizar(palavra);
    let abreviacao = p.len() == 3 || p == "sept";
    let acha = |meses: &[&str; 12]| {
        meses
            .iter()
            .position(|m| *m == p || (abreviacao && m.starts_with(&p[..3.min(p.len())])))
    };
    acha(&MESES)
        .or_else(|| acha(&MESES_EM_INGLES))
        .and_then(|i| u32::try_from(i + 1).ok())
}

/// Um mês escrito por extenso, com o dia e o ano quando o trecho os traz.
struct MesPorExtenso {
    dia: Option<u32>,
    mes: u32,
    ano: Option<i32>,
}

/// Lê `25 de Novembro`, `25 de novembro de 2026`, `Novembro`, `novembro de 2026`, `nov/2026`
/// e `25 nov`; em inglês, `November 20, 2026`, `Nov 20 2026`, `20 November 2026`, `Nov 20` e
/// `Sept 2026`. Qualquer outra coisa (palavra a mais, número solto) é `None`.
fn so_o_mes(bruto: &str) -> Option<MesPorExtenso> {
    let normal = normalizar(bruto);
    let partes: Vec<&str> = normal.split(' ').filter(|p| *p != "de").collect();
    let numero = |p: &str, tamanho: std::ops::RangeInclusive<usize>| {
        tamanho.contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit())
    };
    let (dia, resto) = match partes.first() {
        Some(p) if numero(p, 1..=2) => (Some(p.parse::<u32>().ok()?), &partes[1..]),
        _ => (None, &partes[..]),
    };
    let (nome, dia, ano) = match (dia, resto) {
        (_, [m]) => (*m, dia, None),
        (_, [m, a]) if numero(a, 4..=4) => (*m, dia, Some(a.parse::<i32>().ok()?)),
        // O inglês põe o dia depois do mês: `Nov 20`, `November 20, 2026`.
        (None, [m, d]) if numero(d, 1..=2) => (*m, Some(d.parse::<u32>().ok()?), None),
        (None, [m, d, a]) if numero(d, 1..=2) && numero(a, 4..=4) => (
            *m,
            Some(d.parse::<u32>().ok()?),
            Some(a.parse::<i32>().ok()?),
        ),
        _ => return None,
    };
    Some(MesPorExtenso {
        dia,
        mes: mes_por_nome(nome)?,
        ano,
    })
}

/// Os dias desde 1970-01-01 (calendário gregoriano; para comparar datas).
fn dias_civis(ano: i32, mes: u32, dia: u32) -> i64 {
    let a = i64::from(ano) - i64::from(mes <= 2);
    let era = a.div_euclid(400);
    let ano_da_era = a - era * 400;
    let m = i64::from(mes);
    let d_do_ano = (153 * ((m + 9) % 12) + 2) / 5 + i64::from(dia) - 1;
    let d_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + d_do_ano;
    era * 146_097 + d_da_era - 719_468
}

/// `(ano, mês, dia)` do começo de um `momento` ISO 8601 (`2026-11-20`, `2026-11-20T10:00:00Z`).
fn data_do_momento(momento: &str) -> Option<(i32, u32, u32)> {
    let d = parse_data(momento.trim().get(..10)?)?;
    let mut it = d.split('-');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

/// `AAAA-MM-DD` de `25 de Novembro` (e `25 de Novembro de 2026`). Sem ano, o do `momento`
/// que deixa a data mais perto dele; sem `momento` e sem ano, `None`.
fn data_por_extenso(bruto: &str, momento: Option<&str>) -> Option<String> {
    let m = so_o_mes(bruto)?;
    let dia = m.dia?;
    let valida = |ano: i32| {
        (dia >= 1 && dia <= dias_no_mes(ano, m.mes))
            .then(|| format!("{ano:04}-{:02}-{dia:02}", m.mes))
    };
    if let Some(a) = m.ano {
        return valida(a);
    }
    let (ay, am, ad) = data_do_momento(momento?)?;
    let alvo = dias_civis(ay, am, ad);
    (ay - 1..=ay + 1)
        .filter(|a| dia <= dias_no_mes(*a, m.mes))
        .min_by_key(|a| (dias_civis(*a, m.mes, dia) - alvo).abs())
        .and_then(valida)
}

/// `AAAA-MM` de `Novembro`, `25 de Novembro`, `novembro de 2026`. Sem ano, o do `momento` que
/// deixa o mês mais perto dele; sem `momento` e sem ano, `None`.
fn competencia_por_extenso(bruto: &str, momento: Option<&str>) -> Option<String> {
    let m = so_o_mes(bruto)?;
    if let Some(a) = m.ano {
        return Some(format!("{a:04}-{:02}", m.mes));
    }
    let (ay, am, _) = data_do_momento(momento?)?;
    let alvo = i64::from(ay) * 12 + i64::from(am) - 1;
    let ano = (ay - 1..=ay + 1)
        .min_by_key(|a| (i64::from(*a) * 12 + i64::from(m.mes) - 1 - alvo).abs())?;
    Some(format!("{ano:04}-{:02}", m.mes))
}

/// Inteiro com sinal; pontos são milhar (`1.234` = 1234).
pub fn parse_inteiro(bruto: &str) -> Option<i64> {
    let s = bruto.trim();
    let (neg, resto) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    if resto.is_empty() || !resto.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let n: i64 = resto.replace('.', "").parse().ok()?;
    Some(if neg { -n } else { n })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;

    #[test]
    fn centavos_no_padrao_brasileiro() {
        assert_eq!(parse_centavos("1.234,56"), Some(123_456));
        assert_eq!(parse_centavos("R$ 1.234,56"), Some(123_456));
        assert_eq!(parse_centavos("R$1.234,56"), Some(123_456));
        assert_eq!(parse_centavos("-12,30"), Some(-1230));
        assert_eq!(parse_centavos("R$ -12,30"), Some(-1230));
        assert_eq!(parse_centavos("(12,30)"), Some(-1230));
        assert_eq!(parse_centavos("12,5"), Some(1250));
        assert_eq!(parse_centavos("0,05"), Some(5));
        assert_eq!(parse_centavos("1234.56"), Some(123_456));
        assert_eq!(parse_centavos("1.234"), Some(123_400));
        assert_eq!(parse_centavos("1.234.567,89"), Some(123_456_789));
        assert_eq!(parse_centavos("1234"), Some(123_400));
    }

    #[test]
    fn centavos_rejeita_lixo() {
        for ruim in [
            "", "abc", "12,345", "12,", ",50", "1,2,3", "R$", "12,3x", "--",
        ] {
            assert_eq!(parse_centavos(ruim), None, "{ruim}");
        }
        assert_eq!(parse_centavos("99999999999999999999,00"), None);
    }

    #[test]
    fn datas_e_competencias() {
        assert_eq!(parse_data("06/10/2026").as_deref(), Some("2026-10-06"));
        assert_eq!(parse_data("2026-10-06").as_deref(), Some("2026-10-06"));
        assert_eq!(parse_data("29/02/2028").as_deref(), Some("2028-02-29"));
        assert_eq!(parse_data("29/02/2027"), None);
        assert_eq!(parse_data("31/04/2026"), None);
        assert_eq!(parse_data("6/10/2026"), None);
        assert_eq!(parse_competencia("10/2026").as_deref(), Some("2026-10"));
        assert_eq!(parse_competencia("2026-10").as_deref(), Some("2026-10"));
        assert_eq!(parse_competencia("13/2026"), None);
        assert_eq!(parse_inteiro("-1.234"), Some(-1234));
        assert_eq!(parse_inteiro("12x"), None);
    }

    #[test]
    fn data_e_competencia_por_extenso_com_ano() {
        let f = |fmt: Formato, t: &str| {
            fmt.aplicar_em(t, None).map(|v| match v {
                Valor::Data(d) | Valor::Competencia(d) => d,
                outro => outro.to_string(),
            })
        };
        assert_eq!(
            f(Formato::Data, "25 de Novembro de 2026").as_deref(),
            Some("2026-11-25")
        );
        assert_eq!(
            f(Formato::Competencia, "novembro de 2026").as_deref(),
            Some("2026-11")
        );
        assert_eq!(
            f(Formato::Competencia, "Março/2027").as_deref(),
            Some("2027-03")
        );
        assert_eq!(
            f(Formato::Competencia, "nov/2026").as_deref(),
            Some("2026-11")
        );
        assert_eq!(f(Formato::Data, "31 de abril de 2026"), None);
        assert_eq!(
            f(Formato::Data, "novembro de 2026"),
            None,
            "data pede o dia"
        );
    }

    #[test]
    fn datas_em_ingles_com_ano() {
        for (texto, esperado) in [
            ("November 20, 2026", "2026-11-20"),
            ("November 20 2026", "2026-11-20"),
            ("Nov 20, 2026", "2026-11-20"),
            ("20 November 2026", "2026-11-20"),
            ("20 Nov 2026", "2026-11-20"),
            ("NOVEMBER 5, 2026", "2026-11-05"),
            ("Sept 3, 2026", "2026-09-03"),
            ("September 3, 2026", "2026-09-03"),
            ("Dec 31, 2026", "2026-12-31"),
            ("January 1, 2027", "2027-01-01"),
            ("Feb 29, 2028", "2028-02-29"),
            ("Aug 9 2026", "2026-08-09"),
            ("Oct 9 2026", "2026-10-09"),
            ("May 9 2026", "2026-05-09"),
            ("Apr 9 2026", "2026-04-09"),
        ] {
            assert_eq!(parse_data(texto).as_deref(), Some(esperado), "{texto}");
            assert_eq!(
                Formato::Data.aplicar(texto),
                Some(Valor::Data(esperado.to_owned())),
                "{texto}"
            );
        }
        assert_eq!(parse_data("Feb 29, 2027"), None);
        assert_eq!(parse_data("Nov 31, 2026"), None);
        assert_eq!(parse_data("Novem 20, 2026"), None);
        assert_eq!(parse_data("November 2026"), None, "data pede o dia");
    }

    #[test]
    fn competencia_em_ingles() {
        let c = |t: &str| Formato::Competencia.aplicar(t);
        let comp = |s: &str| Some(Valor::Competencia(s.to_owned()));
        assert_eq!(c("November 20, 2026"), comp("2026-11"));
        assert_eq!(c("20 November 2026"), comp("2026-11"));
        assert_eq!(c("November 2026"), comp("2026-11"));
        assert_eq!(c("Sept 2026"), comp("2026-09"));
        assert_eq!(c("Dec 2026"), comp("2026-12"));
        assert_eq!(c("November 20"), None, "sem ano e sem momento");
        assert_eq!(
            Formato::Competencia.aplicar_em("November 20", Some("2026-11-21")),
            comp("2026-11")
        );
    }

    #[test]
    fn data_em_ingles_sem_ano_usa_o_momento() {
        let f = |t: &str, m: Option<&str>| Formato::Data.aplicar_em(t, m);
        let d = |s: &str| Some(Valor::Data(s.to_owned()));
        assert_eq!(f("November 20", Some("2026-11-21")), d("2026-11-20"));
        assert_eq!(f("Nov 20", Some("2026-11-21")), d("2026-11-20"));
        assert_eq!(f("20 Nov", Some("2026-11-21")), d("2026-11-20"));
        assert_eq!(f("Dec 28", Some("2027-01-05")), d("2026-12-28"));
        assert_eq!(f("Jan 3", Some("2026-12-30T10:00:00Z")), d("2027-01-03"));
        assert_eq!(f("Nov 20", None), None);
    }

    #[test]
    fn meses_dos_dois_idiomas_convivem() {
        // O mesmo número de mês nos dois idiomas.
        for (pt, en, mes) in [
            ("jan", "jan", "01"),
            ("fev", "feb", "02"),
            ("mar", "mar", "03"),
            ("abr", "apr", "04"),
            ("mai", "may", "05"),
            ("jun", "jun", "06"),
            ("jul", "jul", "07"),
            ("ago", "aug", "08"),
            ("set", "sep", "09"),
            ("out", "oct", "10"),
            ("nov", "nov", "11"),
            ("dez", "dec", "12"),
        ] {
            let esperado = Some(Valor::Competencia(format!("2026-{mes}")));
            assert_eq!(
                Formato::Competencia.aplicar(&format!("{pt}/2026")),
                esperado,
                "{pt}"
            );
            assert_eq!(
                Formato::Competencia.aplicar(&format!("{en} 2026")),
                esperado,
                "{en}"
            );
        }
        assert_eq!(
            Formato::Data.aplicar("25 de Novembro de 2026"),
            Some(Valor::Data("2026-11-25".to_owned()))
        );
        assert_eq!(
            Formato::Data.aplicar("25 de dezembro de 2026"),
            Some(Valor::Data("2026-12-25".to_owned()))
        );
        assert_eq!(
            Formato::Competencia.aplicar("March 2026"),
            Some(Valor::Competencia("2026-03".to_owned()))
        );
    }

    #[test]
    fn mes_sem_ano_usa_o_mes_mais_proximo_do_momento() {
        let f = |fmt: Formato, t: &str, m: Option<&str>| {
            fmt.aplicar_em(t, m).map(|v| match v {
                Valor::Data(d) | Valor::Competencia(d) => d,
                outro => outro.to_string(),
            })
        };
        let c = Formato::Competencia;
        assert_eq!(
            f(c, "Novembro", Some("2026-11-20")).as_deref(),
            Some("2026-11")
        );
        assert_eq!(
            f(c, "Dezembro", Some("2026-01-05")).as_deref(),
            Some("2025-12")
        );
        assert_eq!(
            f(c, "janeiro", Some("2026-12-30T10:00:00-03:00")).as_deref(),
            Some("2027-01")
        );
        assert_eq!(
            f(c, "25 de Novembro", Some("2026-11-20")).as_deref(),
            Some("2026-11")
        );
        assert_eq!(
            f(Formato::Data, "25 de Novembro", Some("2026-11-20")).as_deref(),
            Some("2026-11-25")
        );
        assert_eq!(
            f(Formato::Data, "28 de dezembro", Some("2026-01-05")).as_deref(),
            Some("2025-12-28")
        );
        // 29 de fevereiro: só o ano bissexto serve.
        assert_eq!(
            f(Formato::Data, "29 de fevereiro", Some("2027-03-01")).as_deref(),
            Some("2028-02-29")
        );
        assert_eq!(f(c, "Novembro", None), None, "sem momento não há ano");
        assert_eq!(f(c, "Novembro", Some("lixo")), None);
    }

    #[test]
    fn o_motivo_da_falha_diz_por_que_o_mes_sem_ano_nao_saiu() {
        let m = Formato::Competencia.motivo_da_falha("competencia", "25 de Novembro", None);
        assert!(m.contains("sem ano"), "{m}");
        assert!(m.contains("momento"), "{m}");
        let m = Formato::Centavos.motivo_da_falha("valor", "abc", None);
        assert!(m.contains("valor"), "{m}");
        assert!(m.contains("achado mas fora do formato"), "{m}");
    }

    #[test]
    fn normalizar_tira_acento_e_colapsa() {
        assert_eq!(
            normalizar("  DEMONSTRATIVO,  de   Pagamento\n"),
            "demonstrativo de pagamento"
        );
        assert_eq!(normalizar("Ação Única"), "acao unica");
    }

    #[test]
    fn validacao_pega_o_que_esta_errado() {
        let mut d = Definicao::simples("x", "abc");
        assert!(d.validar().is_ok());
        d.versao_do_esquema = 2;
        assert!(d.validar().is_err());
        let mut d = Definicao::simples("", "abc");
        assert!(d.validar().is_err());
        d.tipo_documental = "x".into();
        d.extrair.insert(
            "Valor".into(),
            Extracao {
                regex: "a".into(),
                grupo: 0,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Texto,
            },
        );
        assert!(d.validar().is_err(), "nome com maiúscula");
        d.extrair.clear();
        d.extrair.insert(
            "valor".into(),
            Extracao {
                regex: "(a".into(),
                grupo: 1,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Texto,
            },
        );
        assert!(d.validar().is_err(), "regex quebrado");
        d.extrair.get_mut("valor").unwrap().regex = "a".into();
        assert!(d.validar().is_err(), "grupo 1 não existe");
    }

    #[test]
    fn especificidade_conta_condicoes() {
        let mut d = Definicao::simples("x", "a");
        d.detectar.todos.push(Condicao::Contem("b".into()));
        d.detectar.nenhum.push(Condicao::Contem("c".into()));
        d.detectar.algum.push(Condicao::Contem("d".into()));
        d.detectar.algum.push(Condicao::Contem("e".into()));
        assert_eq!(d.especificidade(), 4);
    }

    #[test]
    fn vincular_ocorrencia_no_json_e_na_geracao() {
        let j = r#"{"tipo_documental":"x","detectar":{"todos":[{"contem":"abc"}]},
            "propor":[{"vincular_ocorrencia":{"compromisso":"servico exemplo","tipo_documento":"comprovante","janela_dias":20}}]}"#;
        let d: Definicao = serde_json::from_str(j).unwrap();
        assert!(d.validar().is_ok());
        let campo = |v: Valor| Campo {
            valor: v,
            de_onde: DeOnde::Dica { nome: "x".into() },
        };
        let mut campos = BTreeMap::new();
        assert!(d.propor[0].gerar(&campos).is_err(), "sem data");
        campos.insert(
            "vencimento".to_owned(),
            campo(Valor::Data("2026-11-20".into())),
        );
        let esperado = Proposta::VincularOcorrencia {
            data: "2026-11-20".into(),
            janela_dias: 20,
            compromisso: "servico exemplo".into(),
            tipo_documento: TipoDoDocumento::Comprovante,
        };
        assert_eq!(
            d.propor[0].gerar(&campos).unwrap(),
            esperado,
            "cai no vencimento"
        );
        campos.insert("data".to_owned(), campo(Valor::Data("2026-11-19".into())));
        assert!(matches!(
            d.propor[0].gerar(&campos),
            Ok(Proposta::VincularOcorrencia { data, .. }) if data == "2026-11-19"
        ));
        // Sem compromisso ou com um tipo que não existe, a definição não vale.
        for ruim in [
            r#"{"vincular_ocorrencia":{"compromisso":"  ","tipo_documento":"outro","janela_dias":3}}"#,
            r#"{"vincular_ocorrencia":{"tipo_documento":"outro","janela_dias":3}}"#,
            r#"{"vincular_ocorrencia":{"compromisso":"a","janela_dias":3}}"#,
            r#"{"vincular_ocorrencia":{"compromisso":"a","tipo_documento":"recibo","janela_dias":3}}"#,
        ] {
            let j = format!(
                r#"{{"tipo_documental":"x","detectar":{{"todos":[{{"contem":"abc"}}]}},"propor":[{ruim}]}}"#
            );
            let ok = serde_json::from_str::<Definicao>(&j)
                .ok()
                .is_some_and(|d| d.validar().is_ok());
            assert!(!ok, "{ruim}");
        }
    }

    #[test]
    fn json_externamente_marcado() {
        let j = r#"{"tipo_documental":"x","detectar":{"todos":[{"contem":"abc"},{"mime":["application/pdf"]}]},
            "propor":["pagar_fatura",{"provar_registro":{"janela_dias":3}}]}"#;
        let d: Definicao = serde_json::from_str(j).unwrap();
        assert_eq!(d.versao_do_esquema, 1);
        assert_eq!(d.aplica_a, vec![TipoDeEntrada::Documento]);
        assert_eq!(d.propor.len(), 2);
        assert!(d.validar().is_ok());
    }
}
