//! A fonte real: a conta do usuário por MTProto (`grammers-client`). O `grammers` é assíncrono;
//! aqui cada chamada pública é síncrona e roda num runtime do tokio que o cliente guarda.
//! Como a biblioteca pode entrar em pânico em respostas inesperadas do servidor, toda chamada
//! passa por `catch_unwind` e vira [`Erro::Fonte`].
//!
//! **Só leitura** (ADR-017): lista conversas e baixa mídias. Nada de enviar, apagar, marcar
//! como lida nem entrar em grupo.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;

use grammers_client::media::Media;
use grammers_client::peer::Peer;
use grammers_client::{Client, SignInError};
use grammers_mtsender::{InvocationError, SenderPool};
use grammers_session::types::{PeerId, PeerRef};
use zeroize::Zeroizing;

use crate::credenciais::Credenciais;
use crate::fonte::{FonteTelegram, vale_baixar};
use crate::modelo::{Avanco, Conversa, Midia, Pedido, Tipo};
use crate::sessao::SessaoMtproto;
use crate::{Erro, Resultado};

/// O que o login pergunta ao usuário (no terminal, `sbf telegram entrar`).
pub trait Perguntas {
    /// O número, no formato internacional (`+55...`).
    ///
    /// # Errors
    /// Falha ao ler a resposta.
    fn numero(&mut self) -> Resultado<String>;
    /// O código que o Telegram mandou ao app.
    ///
    /// # Errors
    /// Falha ao ler a resposta.
    fn codigo(&mut self) -> Resultado<Zeroizing<String>>;
    /// A senha da verificação em duas etapas, se a conta tem.
    ///
    /// # Errors
    /// Falha ao ler a resposta.
    fn senha(&mut self, dica: Option<&str>) -> Resultado<Zeroizing<String>>;
}

/// Como o login terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entrou {
    /// Já havia sessão válida.
    JaEstava,
    /// Entrou agora.
    Agora,
}

fn de_invocacao(e: &InvocationError) -> Erro {
    match e {
        InvocationError::Rpc(r) if r.code == 401 => Erro::PrecisaEntrar,
        outro => Erro::Fonte(outro.to_string()),
    }
}

/// O cliente real.
pub struct ClienteReal {
    rt: tokio::runtime::Runtime,
    client: Client,
    sessao: Arc<SessaoMtproto>,
    credenciais: Credenciais,
    autorizado: Option<bool>,
}

impl std::fmt::Debug for ClienteReal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClienteReal").finish_non_exhaustive()
    }
}

impl Drop for ClienteReal {
    fn drop(&mut self) {
        self.client.disconnect();
    }
}

impl ClienteReal {
    /// Prepara o cliente com a sessão guardada (ou uma nova, para o login). A conexão em si
    /// só abre na primeira chamada.
    ///
    /// # Errors
    /// [`Erro::Fonte`] se o runtime não sobe.
    pub fn novo(credenciais: &Credenciais, sessao: Option<SessaoMtproto>) -> Resultado<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| Erro::Fonte("não deu para subir o runtime".into()))?;
        let sessao = Arc::new(sessao.unwrap_or_default());
        let pool = SenderPool::new(Arc::clone(&sessao), credenciais.api_id);
        let client = Client::new(pool.handle);
        rt.spawn(pool.runner.run());
        Ok(Self {
            rt,
            client,
            sessao,
            credenciais: credenciais.clone(),
            autorizado: None,
        })
    }

    /// A sessão como está agora (chaves novas incluídas), para guardar no chaveiro.
    #[must_use]
    pub fn sessao(&self) -> &SessaoMtproto {
        &self.sessao
    }

    fn executar<T>(&self, trabalho: impl Future<Output = Resultado<T>>) -> Resultado<T> {
        std::panic::catch_unwind(AssertUnwindSafe(|| self.rt.block_on(trabalho)))
            .unwrap_or_else(|_| Err(Erro::Fonte("falha interna do cliente do Telegram".into())))
    }

    /// A sessão guardada ainda vale no Telegram?
    ///
    /// # Errors
    /// Falha de rede.
    pub fn esta_autorizado(&mut self) -> Resultado<bool> {
        if let Some(a) = self.autorizado {
            return Ok(a);
        }
        if !self.sessao.tem_chave() {
            self.autorizado = Some(false);
            return Ok(false);
        }
        let client = self.client.clone();
        let a = self
            .executar(async move { client.is_authorized().await.map_err(|e| de_invocacao(&e)) })?;
        self.autorizado = Some(a);
        Ok(a)
    }

    fn exigir_autorizado(&mut self) -> Resultado<()> {
        if self.esta_autorizado()? {
            Ok(())
        } else {
            Err(Erro::PrecisaEntrar)
        }
    }

    /// O login: número, código recebido no app e, se a conta tem, a senha da verificação em
    /// duas etapas. Nada disso é guardado: só a chave de autorização que resulta
    /// ([`Self::sessao`]).
    ///
    /// # Errors
    /// [`Erro::Login`] com código ou senha recusados; falha de rede.
    pub fn entrar(&mut self, perguntas: &mut dyn Perguntas) -> Resultado<Entrou> {
        if self.esta_autorizado()? {
            return Ok(Entrou::JaEstava);
        }
        let numero = Zeroizing::new(perguntas.numero()?);
        let client = self.client.clone();
        let hash = self.credenciais.api_hash.clone();
        let token = self.executar({
            let client = client.clone();
            let numero = numero.clone();
            async move {
                client
                    .request_login_code(numero.trim(), hash.as_str())
                    .await
                    .map_err(|e| Erro::Login(de_invocacao(&e).to_string()))
            }
        })?;
        let codigo = perguntas.codigo()?;
        let primeiro = self.executar({
            let client = client.clone();
            async move { Ok(client.sign_in(&token, codigo.trim()).await) }
        })?;
        match primeiro {
            Ok(_) => {}
            Err(SignInError::PasswordRequired(mut senha)) => {
                for _ in 0..3 {
                    let dica = senha.hint().map(str::to_owned);
                    let tentativa = perguntas.senha(dica.as_deref())?;
                    let c = client.clone();
                    let r = self.executar(async move {
                        Ok(c.check_password(senha, tentativa.as_bytes()).await)
                    })?;
                    match r {
                        Ok(_) => {
                            self.autorizado = Some(true);
                            return Ok(Entrou::Agora);
                        }
                        Err(SignInError::InvalidPassword(novo)) => senha = novo,
                        Err(e) => return Err(Erro::Login(erro_de_login(&e))),
                    }
                }
                return Err(Erro::Login("senha recusada três vezes".into()));
            }
            Err(e) => return Err(Erro::Login(erro_de_login(&e))),
        }
        self.autorizado = Some(true);
        Ok(Entrou::Agora)
    }

    /// Encerra a sessão no Telegram (o aparelho some da lista de dispositivos).
    ///
    /// # Errors
    /// Falha de rede.
    pub fn sair(&mut self) -> Resultado<()> {
        let client = self.client.clone();
        self.executar(async move {
            client
                .sign_out()
                .await
                .map(|_| ())
                .map_err(|e| de_invocacao(&e))
        })?;
        self.autorizado = Some(false);
        Ok(())
    }
}

fn erro_de_login(e: &SignInError) -> String {
    match e {
        SignInError::InvalidCode => "código recusado".into(),
        SignInError::InvalidPassword(_) => "senha recusada".into(),
        SignInError::SignUpRequired => {
            "esta conta ainda não existe: crie-a primeiro no app oficial".into()
        }
        SignInError::PasswordRequired(_) => "a conta pede a senha".into(),
        SignInError::Other(e) => de_invocacao(e).to_string(),
    }
}

fn tipo_de(peer: &Peer) -> Tipo {
    match peer {
        Peer::User(_) => Tipo::Privada,
        Peer::Channel(_) => Tipo::Canal,
        Peer::Group(_) | Peer::Community(_) => Tipo::Grupo,
    }
}

async fn conversas(client: &Client) -> Resultado<Vec<Conversa>> {
    let mut saida = Vec::new();
    let mut dialogos = client.iter_dialogs();
    while let Some(d) = dialogos.next().await.map_err(|e| de_invocacao(&e))? {
        let Some(id) = d.peer_id().bot_api_dialog_id() else {
            continue;
        };
        saida.push(Conversa {
            id: id.to_string(),
            nome: d
                .peer()
                .name()
                .map_or_else(|| "(sem nome)".to_owned(), str::to_owned),
            tipo: tipo_de(d.peer()),
        });
    }
    Ok(saida)
}

async fn referencia(client: &Client, id: i64) -> Resultado<PeerRef> {
    let alvo = PeerId::from_bot_api_dialog_id(id).ok_or(Erro::ConversaDesconhecida)?;
    let mut dialogos = client.iter_dialogs();
    while let Some(d) = dialogos.next().await.map_err(|e| de_invocacao(&e))? {
        if d.peer_id() == alvo {
            return Ok(d.peer_ref());
        }
    }
    Err(Erro::ConversaDesconhecida)
}

/// O nome e o tipo de arquivo de uma mídia, se vale baixar: foto vira `foto-<id>.jpg`;
/// documento mantém o nome (ou ganha um pela extensão do tipo).
fn descrever(media: &Media, mensagem: i64) -> Option<String> {
    match media {
        Media::Photo(p) => {
            let tamanho = p.size().and_then(|t| u64::try_from(t).ok());
            vale_baixar("foto.jpg", Some("image/jpeg"), tamanho)
                .then(|| format!("foto-{mensagem}.jpg"))
        }
        Media::Document(d) => {
            let mime = d.mime_type();
            let nome = d.name().filter(|n| !n.trim().is_empty()).map_or_else(
                || format!("documento-{mensagem}.{}", extensao_do_tipo(mime)),
                str::to_owned,
            );
            let tamanho = d.size().and_then(|t| u64::try_from(t).ok());
            vale_baixar(&nome, mime, tamanho).then_some(nome)
        }
        _ => None,
    }
}

fn extensao_do_tipo(mime: Option<&str>) -> &'static str {
    match mime {
        Some("application/pdf") => "pdf",
        Some("image/jpeg") => "jpg",
        Some("image/png") => "png",
        Some("text/plain") => "txt",
        Some("text/csv") => "csv",
        Some("application/zip") => "zip",
        _ => "bin",
    }
}

async fn baixar(
    client: &Client,
    pedido: &Pedido,
    pasta: &Path,
    ao_baixar: &mut dyn FnMut(Midia) -> Resultado<()>,
) -> Resultado<Avanco> {
    let id: i64 = pedido
        .conversa
        .parse()
        .map_err(|_| Erro::ConversaDesconhecida)?;
    let peer = referencia(client, id).await?;
    std::fs::create_dir_all(pasta)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700));
    }
    let depois = i32::try_from(pedido.depois_do_id).unwrap_or(i32::MAX);
    let mut mensagens = client.iter_messages(peer).reverse(true).offset_id(depois);
    let mut ate = pedido.depois_do_id;
    let mut feitas = 0_usize;
    while let Some(m) = mensagens.next().await.map_err(|e| de_invocacao(&e))? {
        let mid = i64::from(m.id());
        if mid <= pedido.depois_do_id {
            continue;
        }
        let quando = u64::try_from(m.date().as_second()).unwrap_or(0);
        let Some(media) = m.media() else {
            ate = ate.max(mid);
            continue;
        };
        if pedido.desde_data.is_some_and(|d| quando < d) {
            ate = ate.max(mid);
            continue;
        }
        let Some(nome) = descrever(&media, mid) else {
            ate = ate.max(mid);
            continue;
        };
        if feitas >= pedido.limite {
            break;
        }
        let limpo = sbf_mensageiros::mapeamento::nome_seguro(&nome);
        let caminho = pasta.join(format!("{mid}-{limpo}"));
        let parcial = pasta.join(format!("{mid}-{limpo}.parcial"));
        client.download_media(&media, &parcial).await.map_err(|e| {
            let _ = std::fs::remove_file(&parcial);
            de_invocacao(&e)
        })?;
        std::fs::rename(&parcial, &caminho)?;
        ao_baixar(Midia {
            mensagem: mid,
            quando,
            nome: limpo,
            caminho,
        })?;
        feitas += 1;
        ate = ate.max(mid);
    }
    Ok(Avanco { ate_id: ate })
}

impl FonteTelegram for ClienteReal {
    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>> {
        self.exigir_autorizado()?;
        let client = self.client.clone();
        self.executar(async move { conversas(&client).await })
    }

    fn baixar_midias(
        &mut self,
        pedido: &Pedido,
        pasta: &Path,
        ao_baixar: &mut dyn FnMut(Midia) -> Resultado<()>,
    ) -> Resultado<Avanco> {
        self.exigir_autorizado()?;
        let client = self.client.clone();
        self.executar(async move { baixar(&client, pedido, pasta, ao_baixar).await })
    }
}
