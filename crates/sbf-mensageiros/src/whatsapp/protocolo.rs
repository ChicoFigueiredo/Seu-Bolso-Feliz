//! O protocolo JSON-por-linha entre o SBF e a ponte do WhatsApp.
//!
//! Do SBF para a ponte, [`Comando`], uma linha por comando, sempre com `id`. Da ponte para o
//! SBF, ou uma [`Resposta`] (mesmo `id`) ou um [`Evento`] (sem `id`).
//!
//! **Só leitura:** não existe comando para enviar, responder, reagir, marcar como lida,
//! apagar ou mudar presença. Acrescentar um exige mudar este arquivo e o teste
//! `comandos_sao_so_de_leitura`, que quebra de propósito.

use serde::{Deserialize, Serialize};

/// Um comando para a ponte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Comando<'a> {
    /// Sobe a ponte. `sessao` é a sessão guardada, em base64, já decifrada (só em memória);
    /// sem ela a ponte entra em modo de pareamento e emite o QR.
    Iniciar {
        /// O id do comando.
        id: u64,
        /// A sessão em base64, se já há.
        #[serde(skip_serializing_if = "Option::is_none")]
        sessao: Option<&'a str>,
    },
    /// Lista as conversas.
    Conversas {
        /// O id do comando.
        id: u64,
    },
    /// Mensagens depois de `(desde_quando, desde_id)`, da mais antiga para a mais nova.
    Mensagens {
        /// O id do comando.
        id: u64,
        /// A conversa.
        conversa: &'a str,
        /// Segundos unix do checkpoint.
        desde_quando: u64,
        /// Id do checkpoint, se há.
        #[serde(skip_serializing_if = "Option::is_none")]
        desde_id: Option<&'a str>,
        /// No máximo quantas.
        limite: usize,
    },
    /// Baixa a mídia de uma mensagem para o arquivo `destino`.
    Baixar {
        /// O id do comando.
        id: u64,
        /// A conversa.
        conversa: &'a str,
        /// A mensagem.
        mensagem: &'a str,
        /// O arquivo a criar.
        destino: &'a str,
    },
    /// Desvincula este dispositivo da conta (sai de "Aparelhos conectados").
    Desparear {
        /// O id do comando.
        id: u64,
    },
    /// Encerra a ponte.
    Encerrar {
        /// O id do comando.
        id: u64,
    },
}

/// A resposta a um comando.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Resposta {
    /// O id do comando respondido.
    pub id: u64,
    /// Deu certo?
    pub ok: bool,
    /// O resultado, quando há.
    #[serde(default)]
    pub dados: serde_json::Value,
    /// Código curto da falha (sem conteúdo de conversa).
    #[serde(default)]
    pub erro: Option<String>,
}

/// Um evento espontâneo da ponte.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "evento", rename_all = "snake_case")]
pub enum Evento {
    /// O QR do pareamento (texto; a janela desenha). Renova a cada ~20 s.
    Qr {
        /// O conteúdo do QR.
        codigo: String,
    },
    /// O celular aceitou o vínculo.
    Pareado,
    /// A sessão mudou: guardar cifrada (substitui a anterior).
    Sessao {
        /// A sessão em base64.
        blob: String,
    },
    /// A conexão caiu ou o vínculo foi desfeito pelo celular.
    Desconectado {
        /// Motivo curto.
        #[serde(default)]
        motivo: String,
    },
}

/// Uma linha vinda da ponte.
#[derive(Debug, Clone, PartialEq)]
pub enum Linha {
    /// Resposta a um comando.
    Resposta(Resposta),
    /// Evento.
    Evento(Evento),
}

/// Lê uma linha da ponte; `None` se não é nenhuma das duas formas.
#[must_use]
pub fn ler_linha(linha: &str) -> Option<Linha> {
    if let Ok(r) = serde_json::from_str::<Resposta>(linha) {
        return Some(Linha::Resposta(r));
    }
    serde_json::from_str::<Evento>(linha)
        .ok()
        .map(Linha::Evento)
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;

    /// Os únicos comandos que existem. Se mudar, é decisão do CEO (ADR-017).
    const PERMITIDOS: [&str; 6] = [
        "iniciar",
        "conversas",
        "mensagens",
        "baixar",
        "desparear",
        "encerrar",
    ];

    #[test]
    fn comandos_sao_so_de_leitura() {
        let todos = [
            Comando::Iniciar {
                id: 1,
                sessao: None,
            },
            Comando::Conversas { id: 2 },
            Comando::Mensagens {
                id: 3,
                conversa: "c",
                desde_quando: 0,
                desde_id: None,
                limite: 1,
            },
            Comando::Baixar {
                id: 4,
                conversa: "c",
                mensagem: "m",
                destino: "/tmp/x",
            },
            Comando::Desparear { id: 5 },
            Comando::Encerrar { id: 6 },
        ];
        for c in &todos {
            let v = serde_json::to_value(c).unwrap();
            let nome = v["cmd"].as_str().unwrap();
            assert!(PERMITIDOS.contains(&nome), "comando fora da lista: {nome}");
            for proibido in ["enviar", "send", "responder", "reagir", "apagar", "lida"] {
                assert!(!nome.contains(proibido));
            }
        }
        assert_eq!(todos.len(), PERMITIDOS.len());
    }

    #[test]
    fn linhas_da_ponte_sao_lidas() {
        assert!(matches!(
            ler_linha(r#"{"id":3,"ok":true,"dados":[]}"#),
            Some(Linha::Resposta(r)) if r.id == 3 && r.ok
        ));
        assert!(matches!(
            ler_linha(r#"{"evento":"qr","codigo":"2@abc"}"#),
            Some(Linha::Evento(Evento::Qr { .. }))
        ));
        assert!(matches!(
            ler_linha(r#"{"evento":"desconectado"}"#),
            Some(Linha::Evento(Evento::Desconectado { .. }))
        ));
        assert!(ler_linha("lixo").is_none());
    }
}
