//! O contrato dos scripts (spec 13 §4, ADR-021). Este crate só define; o `sbf-script`
//! executa (Luau, num processo filho).
//!
//! ## O que o script vê
//!
//! Um script Luau define uma função global `transformar(entrada, parametros, ajuda)`:
//!
//! - `entrada`: tabela somente leitura com `tipo_documental`, `canal` e `campos` (os campos
//!   já extraídos: centavos e inteiros como números, datas e competências como texto
//!   `AAAA-MM-DD` / `AAAA-MM`, booleanos). `texto` só existe quando a definição da versão
//!   pede (`script_le_texto: true`).
//! - `parametros`: tabela somente leitura com os parâmetros versionados que a definição
//!   lista, cada um na versão vigente na competência da entrada (`parametros.irrf`).
//! - `ajuda`: funções puras do host — `ajuda.mul_div_arred(a, b, c)` (a·b/c arredondado
//!   metade para longe do zero, em inteiros), `ajuda.centavos("1.234,56")`,
//!   `ajuda.data("06/10/2026")`, `ajuda.normalizar(texto)`.
//!
//! Não existem `io`, `os`, `package`, `require`, `debug`, `load`/`loadstring`,
//! `dofile`/`loadfile`, `getfenv`/`setfenv`, `collectgarbage`, `math.random` nem relógio.
//!
//! ## O que o script devolve
//!
//! Uma tabela: `corresponde` (booleano, obrigatório); `tipo_documental` (texto, opcional);
//! `campos` (nome → valor; nomes terminados em `_centavos` têm de ser inteiros e viram
//! centavos, outros inteiros viram inteiro, texto `AAAA-MM-DD` em nomes que começam por
//! `data` ou `vencimento` vira data, o resto vira texto); `revisar` (booleano);
//! `motivos` e `evidencias` (listas de texto). Qualquer outra coisa é
//! [`ErroDoScript::Retorno`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Os limites de uma execução. O executor encerra o script (e, no processo filho, o próprio
/// processo) quando um deles estoura; o motor registra o erro e segue para a próxima
/// entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limites {
    pub memoria_bytes: usize,
    pub tempo_ms: u64,
    /// Contadas pela interrupção cooperativa do Luau (chamadas e voltas de laço).
    pub instrucoes: u64,
    /// Tamanho máximo do JSON do pedido.
    pub entrada_bytes: usize,
    /// Tamanho máximo do JSON da resposta.
    pub saida_bytes: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            memoria_bytes: 16 * 1024 * 1024,
            tempo_ms: 500,
            instrucoes: 5_000_000,
            entrada_bytes: 1024 * 1024,
            saida_bytes: 256 * 1024,
        }
    }
}

/// Um pedido de execução.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedidoDeScript {
    /// O fonte Luau aprovado (nunca bytecode).
    pub fonte: String,
    /// O checksum da versão, para o executor reaproveitar o que compilou.
    pub checksum: String,
    /// A tabela `entrada` (ver o módulo).
    pub entrada: serde_json::Value,
    /// A tabela `parametros`.
    pub parametros: serde_json::Value,
    pub limites: Limites,
}

/// A resposta, já validada na forma (os tipos dos campos o motor confere).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RespostaDoScript {
    pub corresponde: bool,
    pub tipo_documental: Option<String>,
    pub campos: BTreeMap<String, serde_json::Value>,
    pub revisar: bool,
    pub motivos: Vec<String>,
    pub evidencias: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "erro", content = "detalhe", rename_all = "snake_case")]
pub enum ErroDoScript {
    #[error("erro de sintaxe: {0}")]
    Sintaxe(String),
    #[error("o script não define a função transformar")]
    SemTransformar,
    #[error("passou do tempo")]
    Tempo,
    #[error("passou do limite de memória")]
    Memoria,
    #[error("passou do limite de instruções")]
    Instrucoes,
    #[error("pedido ou resposta grande demais: {0}")]
    Grande(String),
    #[error("retorno inválido: {0}")]
    Retorno(String),
    /// Erro em tempo de execução (o `error(...)` do script, índice de nil...).
    #[error("falha na execução: {0}")]
    Execucao(String),
    /// O processo filho não subiu, morreu ou respondeu fora do protocolo.
    #[error("falha do processo do script: {0}")]
    Processo(String),
}

/// Quem executa scripts. Implementado pelo `sbf-script`; os testes deste crate usam um
/// executor de mentira.
pub trait ExecutorDeScript: Send + Sync + std::fmt::Debug {
    /// Executa `transformar` com os limites do pedido.
    ///
    /// # Errors
    /// [`ErroDoScript`]; nunca entra em pânico por causa do script.
    fn executar(&self, pedido: &PedidoDeScript) -> Result<RespostaDoScript, ErroDoScript>;

    /// Só compila e confere que `transformar` existe, sem rodar (o editor usa).
    ///
    /// # Errors
    /// [`ErroDoScript::Sintaxe`] ou [`ErroDoScript::SemTransformar`].
    fn verificar(&self, fonte: &str) -> Result<(), ErroDoScript>;
}
