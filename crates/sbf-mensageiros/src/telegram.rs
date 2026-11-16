//! Telegram (as conversas do usuário, não o bot): importação do **export JSON do Telegram
//! Desktop** (Configurações › Avançado › Exportar dados do Telegram, formato JSON, com mídia).
//!
//! Por que export e não MTProto: um cliente MTProto (`grammers`, MIT OR Apache-2.0) exige
//! `api_id`/`api_hash` do próprio usuário, login por código e senha em duas etapas, e uma
//! sessão viva; nada disso se testa sem conectar na conta real (proibido aqui). O export é
//! offline, auditável e dá o mesmo material. O adaptador MTProto entra depois pelo mesmo
//! trait [`FonteDeMensagens`]; ver `docs/desktop/mensageiros.md`.
//!
//! Aceita as duas formas do `result.json`: o export completo (`chats.list[]`) e o de uma só
//! conversa (a conversa na raiz). Reexportar a mesma pasta, ou uma pasta mais nova, é
//! seguro: a idempotência é por origem e sha256.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::modelo::{Anexo, Checkpoint, Conversa, Mensagem, Plataforma};
use crate::{Erro, FonteDeMensagens, Resultado};

/// O nome do arquivo do export.
pub const ARQUIVO: &str = "result.json";

#[derive(Debug, Deserialize)]
struct Raiz {
    #[serde(default)]
    chats: Option<Chats>,
    #[serde(flatten)]
    resto: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct Chats {
    #[serde(default)]
    list: Vec<Value>,
}

#[derive(Debug)]
struct ConversaLida {
    conversa: Conversa,
    mensagens: Vec<Mensagem>,
    /// Caminho relativo da mídia, por id de mensagem.
    arquivos: BTreeMap<String, String>,
}

/// Um export aberto: a fonte do Telegram.
#[derive(Debug)]
pub struct ExportDoTelegram {
    raiz: PathBuf,
    conversas: Vec<ConversaLida>,
}

impl ExportDoTelegram {
    /// Abre a pasta do export (a que tem o `result.json`).
    ///
    /// # Errors
    /// [`Erro::NaoPronta`] sem `result.json`; [`Erro::Formato`] se não for um export.
    pub fn abrir(pasta: &Path) -> Resultado<Self> {
        let texto = match std::fs::read_to_string(pasta.join(ARQUIVO)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Erro::NaoPronta),
            Err(e) => return Err(e.into()),
        };
        Self::de_texto(pasta, &texto)
    }

    /// Lê o texto de um `result.json` (a pasta serve para achar a mídia).
    ///
    /// # Errors
    /// [`Erro::Formato`] se não for JSON de export do Telegram.
    pub fn de_texto(pasta: &Path, texto: &str) -> Resultado<Self> {
        let raiz: Raiz = serde_json::from_str(texto)
            .map_err(|_| Erro::Formato("result.json do Telegram".into()))?;
        let mut conversas = Vec::new();
        if let Some(c) = raiz.chats {
            for v in &c.list {
                if let Some(l) = conversa_de(v) {
                    conversas.push(l);
                }
            }
        } else if raiz.resto.contains_key("messages") {
            let v = Value::Object(raiz.resto.into_iter().collect());
            conversas.extend(conversa_de(&v));
        } else {
            return Err(Erro::Formato("result.json do Telegram".into()));
        }
        Ok(Self {
            raiz: pasta.to_path_buf(),
            conversas,
        })
    }

    fn achar(&self, id: &str) -> Option<&ConversaLida> {
        self.conversas.iter().find(|c| c.conversa.id == id)
    }
}

fn texto_do_valor(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(partes) => Some(
            partes
                .iter()
                .map(|p| match p {
                    Value::String(s) => s.as_str(),
                    Value::Object(o) => o.get("text").and_then(Value::as_str).unwrap_or(""),
                    _ => "",
                })
                .collect::<String>(),
        ),
        _ => None,
    }
    .filter(|t| !t.trim().is_empty())
}

fn id_do_valor(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `2024-03-01T10:20:30` (UTC) em segundos unix.
fn segundos_da_data(d: &str) -> Option<u64> {
    let (data, hora) = d.split_once('T')?;
    let mut a = data.split('-').map(|x| x.parse::<i64>().ok());
    let (ano, mes, dia) = (a.next()??, a.next()??, a.next()??);
    let mut h = hora.split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm, ss) = (h.next()??, h.next()??, h.next()??);
    let y = if mes <= 2 { ano - 1 } else { ano };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mes + 9) % 12;
    let doy = (153 * mp + 2) / 5 + dia - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let dias = era * 146_097 + doe - 719_468;
    u64::try_from(dias * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

fn conversa_de(v: &Value) -> Option<ConversaLida> {
    let nome = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("(sem nome)");
    let id = v
        .get("id")
        .and_then(id_do_valor)
        .unwrap_or_else(|| nome.to_owned());
    let tipo = v.get("type").and_then(Value::as_str).unwrap_or("");
    let grupo = !matches!(tipo, "personal_chat" | "saved_messages" | "bot_chat" | "");
    let mut mensagens = Vec::new();
    let mut arquivos = BTreeMap::new();
    for m in v.get("messages")?.as_array()? {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue; // serviço (entrou, saiu, ligação)
        }
        let Some(mid) = m.get("id").and_then(id_do_valor) else {
            continue;
        };
        let quando = m
            .get("date_unixtime")
            .and_then(id_do_valor)
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                m.get("date")
                    .and_then(Value::as_str)
                    .and_then(segundos_da_data)
            })
            .unwrap_or(0);
        let media = m.get("media_type").and_then(Value::as_str).unwrap_or("");
        let sem_interesse = matches!(
            media,
            "sticker"
                | "animation"
                | "voice_message"
                | "video_message"
                | "video_file"
                | "audio_file"
        );
        let arquivo = m
            .get("file")
            .or_else(|| m.get("photo"))
            .and_then(Value::as_str)
            // "(File not included. ...)" quando o export não baixou a mídia.
            .filter(|f| !f.starts_with('(') && !f.is_empty() && !sem_interesse);
        let anexo = arquivo.map(|rel| {
            let nome = m.get("file_name").and_then(Value::as_str).map_or_else(
                || rel.rsplit('/').next().unwrap_or(rel).to_owned(),
                str::to_owned,
            );
            arquivos.insert(mid.clone(), rel.to_owned());
            Anexo {
                nome,
                mime: m
                    .get("mime_type")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                tamanho: m
                    .get("file_size")
                    .or_else(|| m.get("photo_file_size"))
                    .and_then(Value::as_u64),
            }
        });
        mensagens.push(Mensagem {
            id: mid,
            conversa: id.clone(),
            quando,
            texto: m.get("text").and_then(texto_do_valor),
            anexo,
        });
    }
    mensagens.sort_by(|a, b| (a.quando, &a.id).cmp(&(b.quando, &b.id)));
    Some(ConversaLida {
        conversa: Conversa {
            id,
            nome: nome.to_owned(),
            grupo,
        },
        mensagens,
        arquivos,
    })
}

/// O caminho relativo fica dentro da pasta do export? (sem `..`, sem raiz absoluta)
fn relativo_seguro(rel: &str) -> bool {
    Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

impl FonteDeMensagens for ExportDoTelegram {
    fn plataforma(&self) -> Plataforma {
        Plataforma::Telegram
    }

    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>> {
        Ok(self.conversas.iter().map(|c| c.conversa.clone()).collect())
    }

    fn mensagens_novas(
        &mut self,
        conversa: &str,
        desde: &Checkpoint,
        limite: usize,
    ) -> Resultado<Vec<Mensagem>> {
        let Some(c) = self.achar(conversa) else {
            return Ok(Vec::new());
        };
        Ok(c.mensagens
            .iter()
            .filter(|m| desde.antes_de(m))
            .take(limite)
            .cloned()
            .collect())
    }

    fn baixar_anexo(&mut self, mensagem: &Mensagem, destino: &Path) -> Resultado<()> {
        let rel = self
            .achar(&mensagem.conversa)
            .and_then(|c| c.arquivos.get(&mensagem.id))
            .ok_or_else(|| Erro::Fonte("a mensagem não tem mídia no export".into()))?;
        if !relativo_seguro(rel) {
            return Err(Erro::Fonte("caminho de mídia fora do export".into()));
        }
        let origem = self.raiz.join(rel);
        // Um link simbólico dentro do export não pode apontar para fora dele.
        let real = std::fs::canonicalize(&origem)
            .map_err(|_| Erro::Fonte("mídia ausente no export".into()))?;
        let raiz = std::fs::canonicalize(&self.raiz)?;
        if !real.starts_with(&raiz) {
            return Err(Erro::Fonte("caminho de mídia fora do export".into()));
        }
        std::fs::copy(real, destino)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;

    #[test]
    fn data_do_export_vira_segundos_unix() {
        assert_eq!(segundos_da_data("1970-01-01T00:00:00"), Some(0));
        assert_eq!(segundos_da_data("2024-03-01T10:20:30"), Some(1_709_288_430));
        assert_eq!(segundos_da_data("lixo"), None);
    }

    #[test]
    fn texto_em_partes_e_juntado() {
        let v: Value = serde_json::json!(["veja ", {"type":"link","text":"https://x.test"}, " ok"]);
        assert_eq!(
            texto_do_valor(&v).as_deref(),
            Some("veja https://x.test ok")
        );
        assert_eq!(texto_do_valor(&serde_json::json!("")), None);
    }

    #[test]
    fn caminho_relativo_seguro() {
        assert!(relativo_seguro("files/a.pdf"));
        assert!(!relativo_seguro("../a.pdf"));
        assert!(!relativo_seguro("/etc/passwd"));
    }
}
