//! O bot do Telegram por long polling (`getUpdates`, sem webhook).
//!
//! - **Quem pode falar:** só chat_ids autorizados: a lista configurada (`SBF_TELEGRAM_CHATS`)
//!   e os vinculados. Um chat novo se vincula com `/vincular CÓDIGO`, o código de uso único
//!   que `sbf canais vincular-telegram` gera (vale 10 minutos). Qualquer outra mensagem de
//!   um chat desconhecido é ignorada em silêncio.
//! - **Entrada:** documento ou foto vira `source_documents` + `ingestion_jobs` pelo mesmo
//!   caminho do servidor ([`sbf_servidor::documentos::guardar`]), idempotente pelo SHA-256.
//!   O Telegram é uma fonte já filtrada (ADR-014): cai direto na caixa de entrada.
//! - Nada do conteúdo das mensagens vai para o log.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use deadpool_postgres::Pool;
use sbf_servidor::armazenamento::Armazenamento;
use sbf_servidor::documentos::{OrigemDoEnvio, guardar};
use tokio::sync::watch;
use uuid::Uuid;

use crate::confianca::Nivel;
use crate::erro::ErroDeCanal;
use crate::repo::{self, Destinatario};
use crate::telegram::{ClienteTelegram, Mensagem, Update};

const CHAVE_DO_OFFSET: &str = "telegram.offset";

/// Tentativas erradas de código por chat, numa janela.
const TENTATIVAS_DE_CODIGO: usize = 5;
const JANELA_DAS_TENTATIVAS: Duration = Duration::from_secs(10 * 60);

const AJUDA: &str = "Mande um PDF, planilha, OFX, XML ou foto de documento e eu coloco na caixa de entrada.\n\
    /resumo: o que pede atenção agora\n/ajuda: esta mensagem";

pub struct Bot {
    pub banco: Pool,
    pub telegram: ClienteTelegram,
    pub armazenamento: Arc<dyn Armazenamento>,
    /// A lista configurada: fala com o bot sem código.
    pub autorizados: BTreeSet<i64>,
    /// O dono dos dados dos chats da lista configurada.
    pub dono: Uuid,
    pub alertas: Option<crate::agenda::ContextoDeAlertas>,
    tentativas: HashMap<i64, Vec<Instant>>,
    offset: Option<i64>,
}

impl std::fmt::Debug for Bot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bot")
            .field("dono", &self.dono)
            .field("autorizados", &self.autorizados.len())
            .finish_non_exhaustive()
    }
}

/// O que uma mensagem pede.
#[derive(Debug, PartialEq, Eq)]
enum Pedido<'a> {
    Vincular(&'a str),
    Ajuda,
    Resumo,
    Arquivo,
    Nada,
}

fn entender(m: &Mensagem) -> Pedido<'_> {
    if m.document.is_some() || m.photo.as_ref().is_some_and(|p| !p.is_empty()) {
        return Pedido::Arquivo;
    }
    let Some(texto) = m.text.as_deref().map(str::trim) else {
        return Pedido::Nada;
    };
    let (comando, resto) = texto.split_once(char::is_whitespace).unwrap_or((texto, ""));
    // `/comando@nome_do_bot` em grupos.
    let comando = comando.split('@').next().unwrap_or(comando);
    match comando {
        "/vincular" => Pedido::Vincular(resto.trim()),
        // O link `t.me/bot?start=CODIGO` chega como `/start CODIGO`.
        "/start" if !resto.trim().is_empty() => Pedido::Vincular(resto.trim()),
        "/start" | "/ajuda" | "/help" => Pedido::Ajuda,
        "/resumo" => Pedido::Resumo,
        _ => Pedido::Nada,
    }
}

impl Bot {
    pub fn novo(
        banco: Pool,
        telegram: ClienteTelegram,
        armazenamento: Arc<dyn Armazenamento>,
        autorizados: BTreeSet<i64>,
        dono: Uuid,
    ) -> Self {
        Self {
            banco,
            telegram,
            armazenamento,
            autorizados,
            dono,
            alertas: None,
            tentativas: HashMap::new(),
            offset: None,
        }
    }

    /// Quem é o destinatário deste chat, se ele pode falar com o bot.
    async fn autorizar(&self, chat: i64) -> Result<Option<Destinatario>, ErroDeCanal> {
        if let Some(d) = repo::destinatario_do_chat(&self.banco, chat).await? {
            return Ok(Some(d));
        }
        Ok(self.autorizados.contains(&chat).then_some(Destinatario {
            dono: self.dono,
            chat,
            nivel: Nivel::Mascarado,
        }))
    }

    fn estourou_tentativas(&mut self, chat: i64) -> bool {
        let agora = Instant::now();
        let t = self.tentativas.entry(chat).or_default();
        t.retain(|i| agora.duration_since(*i) < JANELA_DAS_TENTATIVAS);
        t.len() >= TENTATIVAS_DE_CODIGO
    }

    async fn responder(&self, d: &Destinatario, tipo: &str, texto: &str) {
        if let Err(e) =
            crate::envio::enviar_registrando(&self.banco, &self.telegram, d, tipo, texto).await
        {
            eprintln!("sbf canais: resposta não saiu: {e}");
        }
    }

    /// Trata uma mensagem. Erros viram log curto (sem conteúdo) e não param o bot.
    pub async fn tratar(&mut self, m: &Mensagem) -> Result<(), ErroDeCanal> {
        let chat = m.chat.id;
        let pedido = entender(m);
        let Some(destinatario) = self.autorizar(chat).await? else {
            // Desconhecido: só o código de vinculação recebe resposta.
            if let Pedido::Vincular(codigo) = pedido {
                self.vincular(chat, codigo).await?;
            }
            return Ok(());
        };
        match pedido {
            Pedido::Vincular(_) => {
                self.responder(&destinatario, "resposta", "Este chat já está vinculado.")
                    .await;
            }
            Pedido::Ajuda => self.responder(&destinatario, "resposta", AJUDA).await,
            Pedido::Resumo => self.resumo(&destinatario).await?,
            Pedido::Arquivo => self.receber_arquivo(&destinatario, m).await,
            Pedido::Nada => {}
        }
        Ok(())
    }

    async fn vincular(&mut self, chat: i64, codigo: &str) -> Result<(), ErroDeCanal> {
        if self.estourou_tentativas(chat) {
            return Ok(());
        }
        let dono = repo::consumir_codigo(&self.banco, codigo).await?;
        let texto = if let Some(dono) = dono {
            repo::vincular(&self.banco, dono, chat).await?;
            "Vinculado. Este chat envia documentos para a caixa de entrada e recebe avisos \
             no nível mascarado (sem números de conta ou cartão)."
        } else {
            self.tentativas
                .entry(chat)
                .or_default()
                .push(Instant::now());
            "Código inválido ou expirado. Gere outro com `sbf canais vincular-telegram`."
        };
        if let Err(e) = self.telegram.enviar_texto(chat, texto).await {
            eprintln!("sbf canais: resposta de vinculação não saiu: {e}");
        }
        Ok(())
    }

    async fn resumo(&self, d: &Destinatario) -> Result<(), ErroDeCanal> {
        let Some(ctx) = &self.alertas else {
            self.responder(d, "resposta", "Os alertas não estão ligados neste serviço.")
                .await;
            return Ok(());
        };
        let livro = repo::carregar_livro(&self.banco, d.dono).await?;
        let (hoje, _) = crate::agenda::relogio_de_sao_paulo(crate::agenda::agora_unix());
        let alertas = crate::alertas::avaliar(&livro, hoje, &ctx.config.limites, &ctx.extras);
        crate::envio::resumir(&self.banco, &self.telegram, d, hoje, &alertas).await
    }

    async fn receber_arquivo(&self, d: &Destinatario, m: &Mensagem) {
        let (file_id, nome) = if let Some(doc) = &m.document {
            let nome = doc.file_name.clone().unwrap_or_else(|| {
                format!(
                    "telegram-{}",
                    doc.file_unique_id.as_deref().unwrap_or("arquivo")
                )
            });
            (doc.file_id.clone(), nome)
        } else if let Some(foto) = m.photo.as_ref().and_then(|p| {
            p.iter().max_by_key(|f| {
                (
                    f.file_size.unwrap_or(0),
                    u64::from(f.width.unwrap_or(0)) * u64::from(f.height.unwrap_or(0)),
                )
            })
        }) {
            let nome = format!(
                "foto-{}.jpg",
                foto.file_unique_id.as_deref().unwrap_or("telegram")
            );
            (foto.file_id.clone(), nome)
        } else {
            return;
        };
        let bytes = match self.telegram.baixar(&file_id).await {
            Ok(b) => b,
            Err(ErroDeCanal::Entrada(m)) => {
                self.responder(d, "resposta", &format!("Não consegui receber: {m}."))
                    .await;
                return;
            }
            Err(e) => {
                eprintln!("sbf canais: download falhou: {e}");
                self.responder(
                    d,
                    "resposta",
                    "Não consegui baixar o arquivo agora. Tente de novo em alguns minutos.",
                )
                .await;
                return;
            }
        };
        let origem = OrigemDoEnvio {
            origem: "telegram",
            detalhe: serde_json::json!({ "origem": "telegram" }),
        };
        let resposta = match guardar(
            &self.banco,
            self.armazenamento.as_ref(),
            d.dono,
            &origem,
            &nome,
            None,
            bytes,
        )
        .await
        {
            Ok(r) if r.ja_existia => "Esse arquivo já estava na caixa de entrada.".to_owned(),
            Ok(_) => "Recebido, vai para a caixa de entrada.".to_owned(),
            Err(e) if e.status.is_client_error() => {
                format!(
                    "Não consegui guardar: {}.",
                    e.corpo.mensagem.trim_end_matches('.')
                )
            }
            Err(e) => {
                eprintln!("sbf canais: gravação falhou: HTTP {}", e.status);
                "Não consegui guardar agora. Tente de novo em alguns minutos.".to_owned()
            }
        };
        self.responder(d, "resposta", &resposta).await;
    }

    async fn ler_offset(&mut self) -> Result<(), ErroDeCanal> {
        if self.offset.is_none() {
            self.offset = repo::ler_estado_do_servico(&self.banco, CHAVE_DO_OFFSET)
                .await?
                .and_then(|v| v.parse().ok());
        }
        Ok(())
    }

    /// Uma volta: um `getUpdates` (espera até `espera` s), trata as mensagens e guarda o
    /// offset. Devolve quantas atualizações vieram.
    pub async fn uma_rodada(&mut self, espera: u64) -> Result<usize, ErroDeCanal> {
        self.ler_offset().await?;
        let atualizacoes: Vec<Update> = self
            .telegram
            .buscar_atualizacoes(self.offset, espera)
            .await?;
        let n = atualizacoes.len();
        for u in atualizacoes {
            // Avança antes de tratar: uma mensagem que quebra o tratamento não trava o bot.
            self.offset = Some(u.update_id + 1);
            if let Some(m) = &u.message
                && let Err(e) = self.tratar(m).await
            {
                eprintln!("sbf canais: mensagem não tratada: {e}");
            }
        }
        if n > 0
            && let Some(o) = self.offset
        {
            repo::gravar_estado_do_servico(&self.banco, CHAVE_DO_OFFSET, &o.to_string()).await?;
        }
        Ok(n)
    }

    /// O laço do bot, até o sinal de parada. Falha de rede espera e tenta de novo.
    pub async fn laco(mut self, mut parar: watch::Receiver<bool>) {
        let mut falhas: u32 = 0;
        loop {
            if *parar.borrow() {
                return;
            }
            let rodada = tokio::select! {
                r = self.uma_rodada(25) => r,
                _ = parar.changed() => continue,
            };
            match rodada {
                Ok(_) => falhas = 0,
                Err(e) => {
                    falhas = falhas.saturating_add(1);
                    eprintln!("sbf canais: getUpdates falhou: {e}");
                    let espera = Duration::from_secs(u64::from(falhas.min(6)) * 5);
                    tokio::select! {
                        () = tokio::time::sleep(espera) => {}
                        _ = parar.changed() => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::telegram::{Chat, Documento};

    fn texto(t: &str) -> Mensagem {
        Mensagem {
            chat: Chat { id: 1 },
            text: Some(t.into()),
            document: None,
            photo: None,
        }
    }

    #[test]
    fn comandos() {
        assert_eq!(
            entender(&texto("/vincular ABC123")),
            Pedido::Vincular("ABC123")
        );
        assert_eq!(entender(&texto("/start XYZ")), Pedido::Vincular("XYZ"));
        assert_eq!(entender(&texto("/start")), Pedido::Ajuda);
        assert_eq!(entender(&texto("/resumo@meu_bot")), Pedido::Resumo);
        assert_eq!(entender(&texto("oi")), Pedido::Nada);
        let mut m = texto("");
        m.text = None;
        m.document = Some(Documento {
            file_id: "f".into(),
            file_unique_id: None,
            file_name: None,
            file_size: None,
        });
        assert_eq!(entender(&m), Pedido::Arquivo);
    }
}
