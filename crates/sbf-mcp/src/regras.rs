//! As ferramentas de regras de reconhecimento (spec 13, D14). Leem o `regras.sqlite` do cofre
//! e, no máximo, criam um **rascunho** com autor `Ia`. **Nunca** ativam, consolidam,
//! desativam nem revertem: isso é do usuário, na janela ou num terminal interativo que
//! confirmou. Por isso não existe, aqui, nenhuma chamada a `ativar`, `consolidar`,
//! `desativar` ou `reverter`.
//!
//! Como em [`crate::plano`], cada chamada grava uma linha em `saida_ia` (destinatário
//! `mcp:<ferramenta>`), sem o conteúdo, e o texto que sai passa pelo nível de confiança do
//! cliente. As datas e as competências continuam legíveis.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::schemars;
use sbf_ia::NivelDeConfianca;
use sbf_regras::catalogo::{Catalogo, NovaVersao, VersaoDeRegra};
use sbf_regras::definicao::Definicao;
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Autor, Entrada, ErroDasRegras, Estado, LIMIAR_DE_CONSOLIDACAO, simulacao};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Os nomes das ferramentas de regras. O servidor só as anuncia com [`Regras`] ligado.
pub const FERRAMENTAS: [&str; 5] = [
    "listar_regras",
    "explicar_regra",
    "testar_regra",
    "simular_regra",
    "propor_regra",
];

/// O máximo de entradas numa simulação.
pub const MAXIMO_DE_ENTRADAS: usize = 200;
const MAX_NOME: usize = 120;
const MAX_MOTIVO: usize = 300;
/// O nome da regra de uma definição avulsa (sem `regra` informada).
const AVULSA: &str = "avulsa";
const MENSAGEM_DE_PROPOSTA: &str =
    "rascunho criado; ativar é só na janela (área Regras de reconhecimento)";

type Resultado = Result<Value, String>;

// ---------------------------------------------------------------------------------------
// Pedidos (os esquemas que o Claude vê)
// ---------------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeRegra {
    /// O id da regra (minúsculas, dígitos e hífen, como `comprovante-pix-banco-exemplo`).
    pub regra: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeTeste {
    /// O id da regra.
    pub regra: String,
    /// O número da versão a testar (veja explicar_regra).
    pub versao: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeSimulacao {
    /// Com `versao`: o id da regra cuja versão salva será simulada no lugar da em vigor.
    /// Com `definicao`: opcional; a regra cuja versão em vigor será substituída pela
    /// definição nova. Sem ele, a definição nova soma às em vigor.
    pub regra: Option<String>,
    /// O número de uma versão salva (com `regra`).
    pub versao: Option<u32>,
    /// Uma definição nova, ainda não salva (objeto JSON; veja o guia em
    /// docs/desktop/regras-de-reconhecimento.md). Alternativa a `regra` + `versao`.
    pub definicao: Option<Value>,
    /// O script Luau da definição nova, se a definição usa um.
    pub script: Option<String>,
    /// As entradas (até 200), cada uma no formato da entrada do motor: `versao_do_esquema`
    /// (1), `id`, `tipo` (documento, registro ou mensagem), `origem` (`canal` e
    /// `id_externo`), `sha256`, `mime`, `nome`, `texto`, `dicas` e `momento`.
    pub entradas: Vec<Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeProposta {
    /// O id da regra. Se a regra já existe, o nome e o tipo de documento têm de ser os
    /// mesmos; se não existe, ela nasce aqui.
    pub regra: String,
    /// O nome para mostrar (até 120 caracteres).
    pub nome: String,
    /// O objeto JSON da definição (veja o guia em docs/desktop/regras-de-reconhecimento.md).
    pub definicao: Value,
    /// O script Luau, se a definição usa um. Confere-se a sintaxe antes de salvar.
    pub script: Option<String>,
    /// Por que esta versão existe, em até 300 caracteres.
    pub motivo: String,
}

// ---------------------------------------------------------------------------------------
// As regras
// ---------------------------------------------------------------------------------------

/// O que o servidor enxerga das regras: o `regras.sqlite`, o `inventario.sqlite` (para
/// `saida_ia`), o executor dos scripts e o nível de confiança deste cliente.
#[derive(Clone, Debug)]
pub struct Regras {
    caminho: PathBuf,
    banco: PathBuf,
    executor: Arc<dyn ExecutorDeScript>,
    nivel: NivelDeConfianca,
}

/// O executor de scripts que usa o próprio binário: `sbf script --servir` num processo
/// filho (o `sbf mcp` roda no mesmo binário).
///
/// # Errors
/// Não achou o caminho do próprio programa.
pub fn executor_do_processo() -> Result<Arc<dyn ExecutorDeScript>, String> {
    let programa = std::env::current_exe()
        .map_err(|_| "não achei o programa para rodar os scripts".to_owned())?;
    Ok(Arc::new(sbf_script::ExecutorFilho::novo(
        programa,
        vec!["script".into(), "--servir".into()],
    )))
}

impl Regras {
    /// As regras sobre `caminho` (o `regras.sqlite`, que tem de existir: nada é criado).
    #[must_use]
    pub fn novo(
        caminho: PathBuf,
        banco: PathBuf,
        executor: Arc<dyn ExecutorDeScript>,
        nivel: NivelDeConfianca,
    ) -> Self {
        Self {
            caminho,
            banco,
            executor,
            nivel,
        }
    }

    /// Os resumos de cada regra e as sugestões de consolidação (só leitura).
    ///
    /// # Errors
    /// Catálogo ausente ou falha do banco.
    pub fn listar(&self) -> Resultado {
        self.executar("listar_regras", Self::listagem)
    }

    /// As versões, a em vigor, a última (definição e script) e os exemplos (só leitura).
    ///
    /// # Errors
    /// Regra inexistente ou falha do banco.
    pub fn explicar(&self, p: &PedidoDeRegra) -> Resultado {
        self.executar("explicar_regra", |s| s.explicacao(p))
    }

    /// Roda os exemplos de uma versão e diz quais passaram (só leitura).
    ///
    /// # Errors
    /// Versão inexistente ou falha do banco.
    pub fn testar(&self, p: &PedidoDeTeste) -> Resultado {
        self.executar("testar_regra", |s| s.testagem(p))
    }

    /// Roda as entradas com a regra em vigor e com a versão (ou a definição nova) e mostra
    /// o que muda. Não grava nada.
    ///
    /// # Errors
    /// Pedido incompleto, entrada inválida, definição ou script inválidos.
    pub fn simular(&self, p: &PedidoDeSimulacao) -> Resultado {
        self.executar("simular_regra", |s| s.simulacao(p))
    }

    /// Salva uma versão **rascunho** com autor `Ia`. Não ativa nada: a versão em vigor
    /// continua a mesma.
    ///
    /// # Errors
    /// Pedido inválido, script que não compila, regra com outro nome ou tipo, ou falha do
    /// banco.
    pub fn propor(&self, p: &PedidoDeProposta) -> Resultado {
        self.executar("propor_regra", |s| s.proposta(p))
    }

    fn executar(&self, ferramenta: &str, f: impl FnOnce(&Self) -> Resultado) -> Resultado {
        let r = f(self).map(|v| mascarar_tudo(v, self.nivel));
        let bytes = match &r {
            Ok(v) => v.to_string().len(),
            Err(e) => e.len(),
        };
        self.registrar(ferramenta, u64::try_from(bytes).unwrap_or(u64::MAX))?;
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

    fn catalogo(&self) -> Result<Catalogo, String> {
        if !self.caminho.is_file() {
            return Err("não achei o catálogo de regras (regras.sqlite do cofre)".to_owned());
        }
        Catalogo::abrir(&self.caminho).map_err(pt)
    }

    fn verificar(&self, fonte: &str) -> Result<(), String> {
        self.executor
            .verificar(fonte)
            .map_err(|e| format!("o script não compila: {e}"))
    }

    fn listagem(&self) -> Resultado {
        let c = self.catalogo()?;
        let regras = c.regras().map_err(pt)?;
        let sugestoes = c.sugestoes_de_consolidacao().map_err(pt)?;
        let itens: Vec<Value> = regras
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "nome": r.nome,
                    "tipo_documental": r.tipo_documental,
                    "versao_em_vigor": r.versao_em_vigor,
                    "estado": r.estado.as_ref().map_or(Value::Null, ok),
                    "versoes": r.versoes,
                    "execucoes": r.execucoes,
                    "retornos": r.retornos_total,
                    "taxa_de_acerto": r.taxa_de_acerto().map(arredonda),
                })
            })
            .collect();
        Ok(json!({
            "total": itens.len(),
            "itens": itens,
            "sugestoes_de_consolidacao": sugestoes
                .iter()
                .map(|(regra, versao)| json!({"regra": regra, "versao": versao}))
                .collect::<Vec<_>>(),
            "limiar_de_consolidacao": LIMIAR_DE_CONSOLIDACAO,
            "mensagem": "sugerir consolidar não consolida: quem consolida é o usuário, na janela (área Regras de reconhecimento)",
        }))
    }

    fn explicacao(&self, p: &PedidoDeRegra) -> Resultado {
        let c = self.catalogo()?;
        let regra = p.regra.trim();
        let versoes = c.versoes(regra).map_err(pt)?;
        let ultima = versoes
            .last()
            .ok_or_else(|| format!("não achei a regra `{regra}`"))?;
        let em_vigor = versoes
            .iter()
            .rev()
            .find(|v| matches!(v.estado, Estado::Ativa | Estado::Consolidada));
        let exemplos = c.exemplos(regra).map_err(pt)?;
        Ok(json!({
            "id": regra,
            "versoes": versoes.iter().map(|v| json!({
                "versao": v.versao,
                "estado": ok(&v.estado),
                "autor": ok(&v.autor),
                "motivo": v.motivo,
                "criada_em": v.criada_em,
                "ativada_em": v.ativada_em,
                "desativada_em": v.desativada_em,
            })).collect::<Vec<_>>(),
            "em_vigor": em_vigor.map(conteudo),
            "ultima": conteudo(ultima),
            "exemplos": exemplos.iter().map(|e| json!({
                "nome": e.nome,
                "esperado": ok(&e.esperado),
            })).collect::<Vec<_>>(),
        }))
    }

    fn testagem(&self, p: &PedidoDeTeste) -> Resultado {
        let c = self.catalogo()?;
        let regra = p.regra.trim();
        let relatorio = c
            .testar(regra, p.versao, Some(self.executor.as_ref()))
            .map_err(pt)?;
        let falhas = relatorio.falhas();
        let mensagem = if relatorio.resultados.is_empty() {
            "a regra não tem exemplos: nada foi provado".to_owned()
        } else if falhas.is_empty() {
            "todos os exemplos passaram".to_owned()
        } else {
            "há exemplos que falharam: esta versão não pode ser ativada".to_owned()
        };
        Ok(json!({
            "regra": regra,
            "versao": p.versao,
            "aprovado": relatorio.aprovado(),
            "falharam": falhas,
            "resultados": relatorio.resultados.iter().map(|r| json!({
                "nome": r.nome,
                "passou": r.passou,
                "detalhe": r.detalhe,
            })).collect::<Vec<_>>(),
            "mensagem": mensagem,
        }))
    }

    fn simulacao(&self, p: &PedidoDeSimulacao) -> Resultado {
        if p.entradas.is_empty() || p.entradas.len() > MAXIMO_DE_ENTRADAS {
            return Err(format!(
                "diga de 1 a {MAXIMO_DE_ENTRADAS} entradas (recebi {})",
                p.entradas.len()
            ));
        }
        let entradas: Vec<Entrada> = p
            .entradas
            .iter()
            .enumerate()
            .map(|(i, v)| {
                serde_json::from_value(v.clone())
                    .map_err(|e| format!("entrada {} inválida: {e}", i + 1))
            })
            .collect::<Result<_, _>>()?;
        let c = self.catalogo()?;
        let atual = Motor::das_vigentes(&c, Some(Arc::clone(&self.executor))).map_err(pt)?;
        let vigentes = c.vigentes().map_err(pt)?;
        let candidatas: Vec<VersaoDeRegra> = match (&p.definicao, p.regra.as_deref(), p.versao) {
            (Some(def), regra, _) => {
                let definicao: Definicao = serde_json::from_value(def.clone())
                    .map_err(|e| format!("definição inválida: {e}"))?;
                if let Some(fonte) = &p.script {
                    self.verificar(fonte)?;
                }
                let id = regra.unwrap_or(AVULSA).trim().to_owned();
                let nova = NovaVersao {
                    regra: id.clone(),
                    nome: id.clone(),
                    definicao,
                    script: p.script.clone(),
                    autor: Autor::Ia,
                    motivo: "simulação, sem salvar".to_owned(),
                };
                // Versão 0: a definição avulsa nunca foi salva.
                let avulsa = VersaoDeRegra::avulsa(nova, 0, Estado::Rascunho).map_err(pt)?;
                let mut mantidas: Vec<VersaoDeRegra> = vigentes
                    .into_iter()
                    .filter(|v| regra.is_none_or(|r| v.regra != r.trim()))
                    .collect();
                mantidas.push(avulsa);
                mantidas
            }
            (None, Some(regra), Some(versao)) => {
                let regra = regra.trim();
                let alvo = c.versao(regra, versao).map_err(pt)?;
                let mut mantidas: Vec<VersaoDeRegra> =
                    vigentes.into_iter().filter(|v| v.regra != regra).collect();
                mantidas.push(alvo);
                mantidas
            }
            _ => {
                return Err(
                    "diga `regra` e `versao` de uma versão salva, ou uma `definicao` nova"
                        .to_owned(),
                );
            }
        };
        let parametros = c.parametros().map_err(pt)?;
        let candidato = Motor::novo(candidatas, parametros, Some(Arc::clone(&self.executor)));
        let diff = simulacao::simular(&atual, &candidato, &entradas);
        Ok(json!({
            "grava": false,
            "entradas": entradas.len(),
            "iguais": diff.iguais,
            "mudaram": diff.mudaram.len(),
            "mudancas": ok(&diff.mudaram),
        }))
    }

    fn proposta(&self, p: &PedidoDeProposta) -> Resultado {
        let regra = texto(&p.regra, "regra", 80)?;
        let nome = texto(&p.nome, "nome", MAX_NOME)?;
        let motivo = texto(&p.motivo, "motivo", MAX_MOTIVO)?;
        let definicao: Definicao = serde_json::from_value(p.definicao.clone())
            .map_err(|e| format!("definição inválida: {e}"))?;
        if let Some(fonte) = &p.script {
            self.verificar(fonte)?;
        }
        let mut c = self.catalogo()?;
        let existente = c.regras().map_err(pt)?.into_iter().find(|r| r.id == regra);
        if let Some(r) = &existente
            && (r.nome != nome || r.tipo_documental != definicao.tipo_documental)
        {
            return Err(
                "a regra já existe com outro nome ou tipo de documento: use o mesmo, ou outro id"
                    .to_owned(),
            );
        }
        let antes = c.versoes(&regra).map_err(pt)?.len();
        let versao = c
            .salvar_rascunho(NovaVersao {
                regra: regra.clone(),
                nome,
                definicao,
                script: p.script.clone(),
                autor: Autor::Ia,
                motivo,
            })
            .map_err(pt)?;
        let criou = c.versoes(&regra).map_err(pt)?.len() > antes;
        let em_vigor = c
            .vigentes()
            .map_err(pt)?
            .into_iter()
            .find(|v| v.regra == regra);
        let mensagem = if criou {
            MENSAGEM_DE_PROPOSTA.to_owned()
        } else {
            format!(
                "o conteúdo é igual à versão {} (estado {}), que já existe: nada novo foi criado",
                versao.versao,
                texto_do_estado(versao.estado)
            )
        };
        Ok(json!({
            "mensagem": mensagem,
            "criou": criou,
            "versao": {
                "regra": versao.regra,
                "versao": versao.versao,
                "estado": ok(&versao.estado),
                "autor": ok(&versao.autor),
            },
            "em_vigor": em_vigor.map(|v| json!({"versao": v.versao, "estado": ok(&v.estado)})),
        }))
    }
}

// ---------------------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------------------

fn pt(e: ErroDasRegras) -> String {
    match e {
        ErroDasRegras::Banco(_) => "falha ao consultar as regras".to_owned(),
        outro => outro.to_string(),
    }
}

fn texto(t: &str, campo: &str, max: usize) -> Result<String, String> {
    let s = t.trim();
    if s.is_empty() {
        return Err(format!("{campo} não pode ficar vazio"));
    }
    if s.chars().count() > max {
        return Err(format!("{campo} passa de {max} caracteres"));
    }
    Ok(s.to_owned())
}

fn texto_do_estado(e: Estado) -> String {
    ok(&e).as_str().unwrap_or("desconhecido").to_owned()
}

fn ok<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn arredonda(t: f64) -> f64 {
    (t * 1000.0).round() / 1000.0
}

fn conteudo(v: &VersaoDeRegra) -> Value {
    json!({
        "versao": v.versao,
        "estado": ok(&v.estado),
        "definicao": ok(&v.definicao),
        "script": v.script,
    })
}

/// `true` só para `AAAA-MM`, `AAAA-MM-DD` e `AAAA-MM-DDTHH:MM:SSZ`, inteiros: datas não são
/// dado pessoal aqui e precisam ficar legíveis. Um número com hífens (cartão, conta) não casa.
fn eh_data(s: &str) -> bool {
    let no_padrao = |modelo: &str| {
        s.len() == modelo.len()
            && s.bytes().zip(modelo.bytes()).all(|(c, m)| {
                if m == b'#' {
                    c.is_ascii_digit()
                } else {
                    c == m
                }
            })
    };
    no_padrao("####-##") || no_padrao("####-##-##") || no_padrao("####-##-##T##:##:##Z")
}

/// Mascara cada texto que sai (CPF, conta e cartão, no nível do cliente). Não mexe em datas.
fn mascarar_tudo(v: Value, nivel: NivelDeConfianca) -> Value {
    match v {
        Value::String(s) if eh_data(&s) => Value::String(s),
        Value::String(s) => Value::String(sbf_indice::perguntar::mascarar(&s, nivel)),
        Value::Array(a) => Value::Array(a.into_iter().map(|x| mascarar_tudo(x, nivel)).collect()),
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, x)| (k, mascarar_tudo(x, nivel)))
                .collect(),
        ),
        outro => outro,
    }
}
