//! Regras de reconhecimento (spec 13, ADR-021).
//!
//! Uma **regra de reconhecimento** diz como reconhecer um tipo de documento ou registro,
//! quais campos tirar dele e o que propor. É dado versionado no `regras.sqlite` do cofre:
//! muda sem recompilar o `sbf`. O algoritmo que não cabe na parte declarativa vai num
//! **script** Luau, executado fora deste crate por um [`script::ExecutorDeScript`] (o
//! `sbf-script` implementa, num processo filho).
//!
//! Este crate é puro: não conhece inventário, planejamento nem janela. Ele recebe uma
//! [`Entrada`] e devolve uma [`Leitura`] com campos rastreáveis e [`Proposta`]s abstratas
//! ("procure um débito de R$ X perto de D"). Quem transforma proposta em sugestão para o
//! usuário (e, numa regra consolidada com as três provas, em conciliação) é o
//! `sbf-reconhecimento`.
//!
//! ## Costura (2026-11-17, coordenador)
//!
//! Os tipos deste arquivo e de [`script`] são o contrato entre os agentes; mudar um deles
//! exige avisar o integrador. O agente do motor acrescenta os módulos:
//!
//! - `definicao`: o formato declarativo (JSON) de uma versão.
//! - `catalogo`: o `regras.sqlite` (migrações, versões imutáveis, estados, exemplos,
//!   execuções, retornos, parâmetros, propostas).
//! - `motor`: casa as versões ativas com uma [`Entrada`], aplica a precedência, chama o
//!   script, monta a [`Leitura`].
//! - `simulacao`: roda duas configurações do motor sobre as mesmas entradas, só leitura,
//!   e devolve o diff.
//! - `rascunho`: monta, sem IA, uma versão-rascunho a partir de uma correção do usuário.
//! - `fabrica`: as regras que vêm com o sbf (D10).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod catalogo;
pub mod definicao;
pub mod fabrica;
pub mod motor;
pub mod rascunho;
pub mod script;
pub mod simulacao;

/// Propostas aceitas seguidas, sem correção, para o sbf sugerir consolidar (D4).
pub const LIMIAR_DE_CONSOLIDACAO: u32 = 5;

/// A versão do esquema da [`Entrada`] e da definição declarativa.
pub const VERSAO_DO_ESQUEMA: u32 = 1;

/// Identificador estável de uma regra, em minúsculas com hífens
/// (`comprovante-pix-banco-exemplo`).
pub type IdDaRegra = String;

// ---------------------------------------------------------------------------------------
// Entrada
// ---------------------------------------------------------------------------------------

/// O que entra no motor, venha de onde vier (o envelope da spec 13, §3.3). O texto inteiro
/// e o arquivo continuam no cofre; o script só recebe o que a definição pede.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrada {
    /// [`VERSAO_DO_ESQUEMA`].
    pub versao_do_esquema: u32,
    /// Identidade estável: o sha256 do arquivo para documentos; `pluggy:<id>` para um
    /// registro da Pluggy; `<canal>:<id externo>` para uma mensagem sem arquivo. Nunca um
    /// hash inventado para o que não tem arquivo.
    pub id: String,
    pub tipo: TipoDeEntrada,
    pub origem: Origem,
    /// O sha256 do conteúdo, quando há arquivo.
    pub sha256: Option<String>,
    /// O tipo real do conteúdo (pela assinatura do arquivo, não pela extensão).
    pub mime: Option<String>,
    /// Só o nome do arquivo, sem a pasta.
    pub nome: Option<String>,
    /// O texto extraído, como saiu do extrator (o motor normaliza para as condições).
    pub texto: Option<String>,
    pub dicas: Dicas,
    /// Quando o fato aconteceu ou chegou, ISO 8601, se a origem diz.
    pub momento: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeEntrada {
    /// Um arquivo: PDF, imagem, texto.
    Documento,
    /// Um registro de uma conexão sem arquivo: a transação da Pluggy, a linha de um extrato.
    Registro,
    /// Uma mensagem (e-mail, Telegram) sem anexo.
    Mensagem,
}

/// Uma origem da entrada. O mesmo conteúdo achado em dois canais é uma entrada com duas
/// origens (o inventário acumula); o motor recebe uma de cada vez.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origem {
    pub canal: Canal,
    /// O id no canal (id da mensagem, da transação, do anexo), quando há.
    pub id_externo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Canal {
    /// Solto na janela.
    Arrastado,
    /// Uma pasta vigiada ou um disco do inventário.
    Pasta,
    /// Já estava no acervo do cofre.
    Cofre,
    Gmail,
    Telegram,
    Pluggy,
    /// Mandado por uma ferramenta do MCP.
    Mcp,
    /// Digitado na janela.
    Manual,
}

/// O que a origem já sabe sem ler o conteúdo (a Pluggy traz valor, data e descrição).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dicas {
    /// O apelido da conta no planejamento ou o id da conta na conexão.
    pub conta: Option<String>,
    /// Negativo é saída.
    pub valor_centavos: Option<i64>,
    /// `AAAA-MM-DD`.
    pub data: Option<String>,
    /// A descrição do registro no banco ("PIX ENVIADO FULANO").
    pub descricao: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Campos
// ---------------------------------------------------------------------------------------

/// Um valor extraído. Dinheiro sempre em centavos inteiros.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "valor", rename_all = "snake_case")]
pub enum Valor {
    Texto(String),
    Centavos(i64),
    /// `AAAA-MM-DD`.
    Data(String),
    /// `AAAA-MM`.
    Competencia(String),
    Inteiro(i64),
    Booleano(bool),
}

impl std::fmt::Display for Valor {
    /// O valor como a pessoa lê: `R$ 2.310,45`, `12/10/2026`, `10/2026`, `sim`/`não`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Valor::Texto(t) => f.write_str(t),
            Valor::Centavos(c) => {
                let n = c.unsigned_abs();
                let digitos = (n / 100).to_string();
                let mut agrupado = String::new();
                for (i, d) in digitos.chars().enumerate() {
                    if i > 0 && (digitos.len() - i).is_multiple_of(3) {
                        agrupado.push('.');
                    }
                    agrupado.push(d);
                }
                let sinal = if *c < 0 { "-" } else { "" };
                write!(f, "{sinal}R$ {agrupado},{:02}", n % 100)
            }
            Valor::Data(d) => match d.split('-').collect::<Vec<_>>().as_slice() {
                [a, m, dia] => write!(f, "{dia}/{m}/{a}"),
                _ => f.write_str(d),
            },
            Valor::Competencia(c) => match c.split_once('-') {
                Some((a, m)) => write!(f, "{m}/{a}"),
                None => f.write_str(c),
            },
            Valor::Inteiro(n) => write!(f, "{n}"),
            Valor::Booleano(b) => f.write_str(if *b { "sim" } else { "não" }),
        }
    }
}

/// De onde um campo veio (AC-LUA-08): nada derivado sem origem.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "de", rename_all = "snake_case")]
pub enum DeOnde {
    /// Um trecho do texto: posições em bytes no texto **como recebido** na [`Entrada`].
    Trecho { inicio: usize, fim: usize },
    /// Uma dica da origem (`valor_centavos`, `data`, `descricao`...).
    Dica { nome: String },
    /// Um leitor nativo do sbf (`contracheque`).
    Leitor { leitor: String },
    /// Calculado pelo script da versão, a partir dos campos listados.
    Script { a_partir_de: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campo {
    pub valor: Valor,
    pub de_onde: DeOnde,
}

// ---------------------------------------------------------------------------------------
// Regras e versões
// ---------------------------------------------------------------------------------------

/// O estado de uma versão (D4). Só uma versão por regra fica ativa ou consolidada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    /// Nunca roda na ingestão; só em teste e simulação.
    Rascunho,
    /// Reconhece e propõe; nada vira conciliação sem o clique.
    Ativa,
    /// Como ativa, e concilia sozinha quando as três provas batem exatamente.
    Consolidada,
    /// Foi ativa e deixou de ser (rollback, versão nova, desligada). Fica no histórico.
    Desativada,
}

/// Quem escreveu uma versão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autor {
    Usuario,
    /// Proposta pela IA (MCP). Nasce rascunho, sempre.
    Ia,
    /// Veio com o sbf (D10).
    Fabrica,
    /// Montada sem IA a partir de uma correção na caixa de entrada (D7).
    Correcao,
}

/// Quem pede uma ação no catálogo. Ativar, consolidar, desativar e reverter exigem
/// [`Quem::Usuario`] (D14); o catálogo recusa os outros com [`ErroDasRegras::NaoPermitido`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quem {
    /// O usuário, na janela ou num terminal interativo que confirmou.
    Usuario,
    /// A IA, pelo MCP.
    Ia,
    /// O próprio sbf (semear a fábrica, rebaixar uma consolidada depois de um Desfazer).
    Sistema,
}

/// Uma proposta abstrata: o que procurar, não o que gravar (AC-LUA-06). Não existe
/// variante para lançar, pagar ou criar movimentação; quem aplica é o Rust de quem chama.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "proposta", rename_all = "snake_case")]
pub enum Proposta {
    /// O documento prova um registro do banco que já existe (o comprovante do Pix prova o
    /// débito). Nunca cria gasto novo.
    ProvarRegistro {
        valor_centavos: i64,
        data: String,
        /// Folga em dias em volta de `data`.
        janela_dias: u32,
        conta: Option<String>,
    },
    /// Conciliar a ocorrência de um compromisso do mês com este documento ou registro.
    ConciliarOcorrencia {
        valor_centavos: i64,
        data: String,
        janela_dias: u32,
        /// Um nome para achar o compromisso ("Energia"), se a regra sabe.
        compromisso: Option<String>,
    },
    /// O débito no banco paga uma fatura de cartão: é pagamento de fatura, não gasto
    /// (os itens da fatura já são os gastos). D11.
    PagarFatura {
        valor_centavos: i64,
        vencimento: String,
        cartao: Option<String>,
    },
}

/// Uma regra reconheceu a entrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconhecimento {
    pub regra: IdDaRegra,
    pub versao: u32,
    /// O checksum da versão (definição + script).
    pub checksum: String,
    pub tipo_documental: String,
    pub estado: Estado,
    /// Ordenado pelo nome: a serialização é estável (AC-LUA-04).
    pub campos: BTreeMap<String, Campo>,
    /// Um leitor nativo que ainda precisa rodar (`contracheque`): quem chama roda o leitor
    /// e devolve os campos ao motor para o script (ver o módulo `motor`).
    pub leitor: Option<String>,
    /// Vai para a caixa de entrada como "Precisa revisar".
    pub revisar: bool,
    /// Por que revisar, por que esta regra venceu, avisos do script. Sem conteúdo do
    /// documento além do necessário para o usuário entender.
    pub motivos: Vec<String>,
    /// Rótulos das evidências usadas (`rubrica_irrf`, `tabela:irrf-2026-01`).
    pub evidencias: Vec<String>,
    pub propostas: Vec<Proposta>,
    /// Chave de idempotência: sha256 de (entrada.id, regra, versão, checksum, checksums dos
    /// parâmetros usados). Reprocessar não duplica proposta (AC-LUA-04, AC-LUA-09).
    pub chave: String,
}

/// O que o motor decidiu sobre uma entrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decisao", rename_all = "snake_case")]
pub enum Decisao {
    Reconhecido(Box<Reconhecimento>),
    /// Duas ou mais regras empatadas na precedência discordam: nada é escrito, vai para
    /// revisão (AC-LUA-03).
    Ambiguo {
        candidatas: Vec<Reconhecimento>,
        motivo: String,
    },
    /// Nenhuma regra ativa se aplica: quem chama segue para os leitores nativos e a IA.
    Nenhuma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultadoDaExecucao {
    Reconhecido,
    NaoSeAplica,
    /// Detectou, mas a extração ou o script falhou; a entrada não trava a fila.
    Erro,
}

/// Uma regra avaliada para uma entrada: entra no Rastro e na tabela de execuções, sem
/// texto do documento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execucao {
    pub regra: IdDaRegra,
    pub versao: u32,
    pub resultado: ResultadoDaExecucao,
    /// Só a CATEGORIA do erro (`sintaxe`, `tempo`, `regex`...), nunca texto livre: é o que
    /// vai para a tabela `execucao`, que não guarda texto do documento.
    pub motivo: Option<String>,
    /// A mensagem completa do erro, só em memória (o editor mostra ao testar um rascunho);
    /// nunca é gravada, pois pode conter texto do documento.
    #[serde(default, skip)]
    pub detalhe: Option<String>,
    pub duracao_us: u64,
}

/// O resultado do motor para uma entrada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leitura {
    pub entrada: String,
    pub decisao: Decisao,
    pub execucoes: Vec<Execucao>,
}

/// O que o usuário fez com uma proposta de uma regra (alimenta a consolidação, D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retorno {
    Aceito,
    /// Aceitou depois de mudar algum campo: zera a sequência.
    Corrigido,
    Rejeitado,
    /// Desfez uma conciliação automática: zera a sequência e rebaixa a consolidada.
    Desfeito,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDasRegras {
    #[error("falha do banco de regras: {0}")]
    Banco(String),
    #[error("definição inválida: {0}")]
    Definicao(String),
    /// A ativação foi bloqueada porque exemplos falharam (D12); os nomes deles.
    #[error("exemplos falharam: {}", .0.join(", "))]
    Exemplos(Vec<String>),
    #[error("não permitido: {0}")]
    NaoPermitido(String),
    #[error("não encontrada: {0}")]
    NaoEncontrada(String),
    #[error("script: {0}")]
    Script(#[from] script::ErroDoScript),
}

#[cfg(test)]
mod testes_do_valor {
    use super::Valor;

    #[test]
    fn o_valor_se_le_como_a_pessoa_escreve() {
        assert_eq!(Valor::Centavos(231_045).to_string(), "R$ 2.310,45");
        assert_eq!(Valor::Centavos(5).to_string(), "R$ 0,05");
        assert_eq!(Valor::Centavos(-125_000).to_string(), "-R$ 1.250,00");
        assert_eq!(Valor::Centavos(100_000_000).to_string(), "R$ 1.000.000,00");
        assert_eq!(Valor::Data("2026-10-12".into()).to_string(), "12/10/2026");
        assert_eq!(Valor::Competencia("2026-10".into()).to_string(), "10/2026");
        assert_eq!(Valor::Inteiro(7).to_string(), "7");
        assert_eq!(Valor::Booleano(false).to_string(), "não");
        assert_eq!(
            Valor::Texto("Fulano de Tal".into()).to_string(),
            "Fulano de Tal"
        );
        // Formato que não reconhece vai como veio.
        assert_eq!(Valor::Data("amanhã".into()).to_string(), "amanhã");
    }
}
