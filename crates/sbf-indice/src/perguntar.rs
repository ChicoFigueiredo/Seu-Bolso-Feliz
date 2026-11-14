//! A área Perguntar (spec 10, RG-03): a pergunta busca os `k` trechos mais próximos no
//! índice local; **só esses trechos** vão ao destinatário (o motor de IA), mascarados no
//! nível de confiança dele; o que saiu fica em `saida_ia` (por [`Registro`]); a resposta
//! cita os trechos, e cada citação aponta para o item de onde ele veio.

use sbf_ia::{Degrau, NivelDeConfianca, Pedido, Provedor, Registro, Saida, agora_rfc3339};
use serde_json::json;

use crate::busca::Busca;
use crate::erro::{ErroDoIndice, Resultado};
use crate::indice::ItemRef;

/// Versão do prompt (entra na chave de cache do motor e em `saida_ia` pelo registro).
pub const PROMPT_VERSAO: &str = "perguntar-v1";
/// O que vai em `saida_ia.destinatario`.
pub const DESTINATARIO: &str = "perguntar";
/// Quantos trechos vão por padrão.
pub const K_PADRAO: usize = 6;

const INSTRUCAO: &str = "Você responde perguntas sobre as finanças pessoais do usuário, em português. \
Use SOMENTE os trechos numerados que vêm na entrada; não use conhecimento externo e não invente. \
Se os trechos não bastam, diga isso. Cite os trechos usados pelo número em `citacoes`. \
Responda em até 6 frases.";

/// Uma citação: o número que a resposta usou, o item e o trecho (texto original, local).
#[derive(Debug, Clone, PartialEq)]
pub struct Citacao {
    pub numero: usize,
    pub item: ItemRef,
    pub trecho: String,
}

/// A resposta de Perguntar.
#[derive(Debug, Clone, PartialEq)]
pub struct Resposta {
    pub texto: String,
    pub citacoes: Vec<Citacao>,
    /// Quantos trechos saíram para o destinatário (zero: nada saiu, não houve chamada).
    pub trechos_enviados: usize,
}

/// Troca por `•••` cada sequência de seis ou mais dígitos (com espaço, ponto ou hífen
/// entre eles); no nível parcial, os 4 últimos ficam. Completo não mexe. É a mesma regra do
/// `sbf-canais` (`confianca::proteger`), copiada para não puxar o Postgres e o tokio.
#[must_use]
pub fn mascarar(texto: &str, nivel: NivelDeConfianca) -> String {
    if nivel == NivelDeConfianca::Completo {
        return texto.to_owned();
    }
    let chars: Vec<char> = texto.chars().collect();
    let mut saida = String::with_capacity(texto.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            saida.push(chars[i]);
            i += 1;
            continue;
        }
        let inicio = i;
        let mut fim = i;
        let mut j = i;
        while j < chars.len() {
            if chars[j].is_ascii_digit() {
                fim = j + 1;
                j += 1;
            } else if matches!(chars[j], ' ' | '.' | '-')
                && chars.get(j + 1).is_some_and(char::is_ascii_digit)
            {
                j += 1;
            } else {
                break;
            }
        }
        let trecho = &chars[inicio..fim];
        let digitos: Vec<char> = trecho
            .iter()
            .copied()
            .filter(char::is_ascii_digit)
            .collect();
        if digitos.len() >= 6 {
            saida.push_str("•••");
            if nivel == NivelDeConfianca::Parcial {
                saida.extend(&digitos[digitos.len() - 4..]);
            }
        } else {
            saida.extend(trecho);
        }
        i = fim;
    }
    saida
}

fn esquema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "resposta": {"type": "string"},
            "citacoes": {"type": "array", "items": {"type": "integer"}}
        },
        "required": ["resposta", "citacoes"],
        "additionalProperties": false
    })
}

/// Pergunta ao motor com os `k` trechos mais próximos.
///
/// # Errors
/// Falha da busca, do motor ou do registro; resposta fora do formato.
pub fn perguntar(
    busca: &dyn Busca,
    provedor: &dyn Provedor,
    registro: &dyn Registro,
    nivel: NivelDeConfianca,
    pergunta: &str,
    k: usize,
) -> Resultado<Resposta> {
    let pergunta = pergunta.trim();
    if pergunta.is_empty() {
        return Err(ErroDoIndice::Outro("a pergunta está vazia".into()));
    }
    let achados = busca.parecidos(pergunta, k.max(1), None)?;
    if achados.is_empty() {
        return Ok(Resposta {
            texto: "Não achei nada no índice para responder. Ele pode estar vazio ou ainda sendo montado."
                .to_owned(),
            citacoes: Vec::new(),
            trechos_enviados: 0,
        });
    }
    let mut entrada = format!("Pergunta: {}\n\nTrechos:\n", mascarar(pergunta, nivel));
    for (i, a) in achados.iter().enumerate() {
        entrada.push_str(&format!("[{}] {}\n", i + 1, mascarar(&a.texto, nivel)));
    }
    let pedido = Pedido {
        prompt_versao: PROMPT_VERSAO.to_owned(),
        instrucao: INSTRUCAO.to_owned(),
        entrada,
        esquema: esquema(),
        degrau: Degrau::Rapido,
        max_tokens_saida: 800,
    };
    let resposta = provedor.pedir(&pedido)?;
    registro.registrar_saida(&Saida {
        destinatario: DESTINATARIO.to_owned(),
        provedor: provedor.nome().to_owned(),
        modelo: resposta.modelo.clone(),
        nivel,
        bytes_enviados: (pedido.instrucao.len() + pedido.entrada.len()) as u64,
        consumo: resposta.consumo,
        quando: agora_rfc3339(),
    })?;
    let texto = resposta
        .json
        .get("resposta")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErroDoIndice::Resposta("falta `resposta`".into()))?
        .to_owned();
    let mut citacoes: Vec<Citacao> = Vec::new();
    if let Some(lista) = resposta.json.get("citacoes").and_then(|v| v.as_array()) {
        for n in lista.iter().filter_map(serde_json::Value::as_u64) {
            let n = n as usize;
            // Citação fora dos trechos enviados é invenção do modelo: descarta.
            if n >= 1 && n <= achados.len() && !citacoes.iter().any(|c| c.numero == n) {
                let a = &achados[n - 1];
                citacoes.push(Citacao {
                    numero: n,
                    item: a.item.clone(),
                    trecho: a.texto.clone(),
                });
            }
        }
    }
    Ok(Resposta {
        texto,
        citacoes,
        trechos_enviados: achados.len(),
    })
}

/// Um [`Registro`] em memória (demonstração e testes): guarda as saídas e o cache.
#[derive(Debug, Default)]
pub struct RegistroEmMemoria {
    saidas: std::sync::Mutex<Vec<Saida>>,
    cache: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

impl RegistroEmMemoria {
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// As saídas registradas até agora.
    #[must_use]
    pub fn saidas(&self) -> Vec<Saida> {
        self.saidas.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Registro for RegistroEmMemoria {
    fn registrar_saida(&self, saida: &Saida) -> sbf_ia::ResultadoDeIa<()> {
        self.saidas
            .lock()
            .map_err(|_| sbf_ia::ErroDeIa::Outro("registro envenenado".into()))?
            .push(saida.clone());
        Ok(())
    }
    fn ler_cache(&self, chave: &str) -> sbf_ia::ResultadoDeIa<Option<serde_json::Value>> {
        Ok(self.cache.lock().ok().and_then(|c| c.get(chave).cloned()))
    }
    fn gravar_cache(&self, chave: &str, json: &serde_json::Value) -> sbf_ia::ResultadoDeIa<()> {
        if let Ok(mut c) = self.cache.lock() {
            c.insert(chave.to_owned(), json.clone());
        }
        Ok(())
    }
}

/// O serviço que a janela usa: uma pergunta, uma resposta. O real é montado no `sbf` (índice,
/// embedder e provedor); a demonstração usa o fictício. Roda numa thread da janela: pode
/// demorar (modelo, rede do provedor).
pub trait ServicoDePerguntar: Send {
    /// # Errors
    /// Texto para mostrar ao usuário.
    fn perguntar(&mut self, pergunta: &str) -> Result<Resposta, String>;

    /// O modelo de busca já está na máquina? Sem ele, a janela avisa do download antes.
    fn modelo_pronto(&self) -> bool {
        true
    }

    /// Baixa o modelo de busca (~120 MB, uma vez). Só o app real, depois do aviso na janela.
    ///
    /// # Errors
    /// Texto para mostrar ao usuário.
    fn baixar_modelo(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Quantos trechos o índice tem hoje (`None`: não se sabe).
    fn trechos_no_indice(&self) -> Option<usize> {
        None
    }
}
