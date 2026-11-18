//! A sessão MTProto: o que o `grammers` precisa para voltar a falar com o Telegram sem pedir
//! o código de novo (a chave de autorização de cada datacenter). Ela é **serializada e
//! guardada só no chaveiro** (rótulo `mensageiro:telegram`): nunca em arquivo solto, log ou
//! mensagem de erro. Perder o chaveiro equivale a sair: `sbf telegram entrar` de novo.
//!
//! O cache de pessoas e conversas não é guardado (só o próprio usuário): cada coleta o refaz
//! ao listar as conversas, e a sessão fica pequena.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use grammers_session::{BoxFuture, Session, SessionData};
use sbf_chaveiro::{Chaveiro, Segredo};
use serde::{Deserialize, Serialize};

use crate::{Erro, Resultado};

/// O nome do segredo no chaveiro (`mensageiro:telegram`).
pub const NOME_NO_CHAVEIRO: &str = "telegram";
/// As credenciais do aplicativo (api id e hash), para o worker não depender do `.env`
/// (`mensageiro:telegram-api`).
pub const NOME_DAS_CREDENCIAIS: &str = "telegram-api";

const VERSAO: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Dump {
    versao: u32,
    home_dc: i32,
    /// Só os datacenters que têm chave de autorização.
    dc_options: Vec<DcOption>,
    /// Só o próprio usuário.
    eu: Vec<PeerInfo>,
}

/// O erro da sessão em memória.
#[derive(Debug)]
pub struct ErroDaSessao;

impl std::fmt::Display for ErroDaSessao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a sessão está inutilizada")
    }
}

impl std::error::Error for ErroDaSessao {}

/// A sessão em memória, que sabe se exportar e se restaurar.
pub struct SessaoMtproto(Mutex<SessionData>);

impl std::fmt::Debug for SessaoMtproto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nunca mostra chave nem identidade.
        f.debug_struct("SessaoMtproto").finish_non_exhaustive()
    }
}

impl Default for SessaoMtproto {
    fn default() -> Self {
        Self(Mutex::new(SessionData::default()))
    }
}

impl SessaoMtproto {
    fn dados(&self) -> Result<MutexGuard<'_, SessionData>, ErroDaSessao> {
        self.0.lock().map_err(|_| ErroDaSessao)
    }

    /// Há alguma chave de autorização (alguma vez falou com o Telegram)?
    #[must_use]
    pub fn tem_chave(&self) -> bool {
        self.dados()
            .is_ok_and(|d| d.dc_options.values().any(|o| o.auth_key.is_some()))
    }

    /// Exporta para texto (JSON). **É um segredo**: só vai para o chaveiro.
    ///
    /// # Errors
    /// [`Erro::Formato`] se a sessão está inutilizada.
    pub fn exportar(&self) -> Resultado<String> {
        let d = self
            .dados()
            .map_err(|_| Erro::Formato("sessão inutilizada".into()))?;
        let dump = Dump {
            versao: VERSAO,
            home_dc: d.home_dc,
            dc_options: d
                .dc_options
                .values()
                .filter(|o| o.auth_key.is_some())
                .cloned()
                .collect(),
            eu: d
                .peer_infos
                .values()
                .filter(|p| {
                    matches!(
                        p,
                        PeerInfo::User {
                            is_self: Some(true),
                            ..
                        }
                    )
                })
                .cloned()
                .collect(),
        };
        serde_json::to_string(&dump).map_err(|_| Erro::Formato("sessão".into()))
    }

    /// Restaura do texto de [`Self::exportar`].
    ///
    /// # Errors
    /// [`Erro::Formato`] se o texto não é uma sessão (sem dizer o que havia nele).
    pub fn restaurar(texto: &str) -> Resultado<Self> {
        let dump: Dump =
            serde_json::from_str(texto).map_err(|_| Erro::Formato("sessão ilegível".into()))?;
        if dump.versao != VERSAO {
            return Err(Erro::Formato("sessão de outra versão".into()));
        }
        let mut dados = SessionData {
            home_dc: dump.home_dc,
            ..SessionData::default()
        };
        for o in dump.dc_options {
            dados.dc_options.insert(o.id, o);
        }
        let peers: HashMap<PeerId, PeerInfo> = dump.eu.into_iter().map(|p| (p.id(), p)).collect();
        dados.peer_infos = peers;
        Ok(Self(Mutex::new(dados)))
    }
}

impl Session for SessaoMtproto {
    type Error = ErroDaSessao;

    fn home_dc_id(&self) -> Result<i32, ErroDaSessao> {
        Ok(self.dados()?.home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), ErroDaSessao>> {
        Box::pin(async move {
            self.dados()?.home_dc = dc_id;
            Ok(())
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, ErroDaSessao> {
        Ok(self.dados()?.dc_options.get(&dc_id).cloned())
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), ErroDaSessao>> {
        let dc_option = dc_option.clone();
        Box::pin(async move {
            self.dados()?.dc_options.insert(dc_option.id, dc_option);
            Ok(())
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, ErroDaSessao>> {
        Box::pin(async move { Ok(self.dados()?.peer_infos.get(&peer).cloned()) })
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), ErroDaSessao>> {
        Box::pin(async move {
            self.dados()?
                .peer_infos
                .entry(peer.id())
                .or_insert_with(|| peer.clone())
                .extend_info(&peer);
            Ok(())
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, ErroDaSessao>> {
        Box::pin(async move { Ok(self.dados()?.updates_state.clone()) })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), ErroDaSessao>> {
        Box::pin(async move {
            let mut dados = self.dados()?;
            match update {
                UpdateState::All(estado) => dados.updates_state = estado,
                UpdateState::Primary { pts, date, seq } => {
                    dados.updates_state.pts = pts;
                    dados.updates_state.date = date;
                    dados.updates_state.seq = seq;
                }
                UpdateState::Secondary { qts } => dados.updates_state.qts = qts,
                UpdateState::Channel { id, pts } => {
                    dados.updates_state.channels.retain(|c| c.id != id);
                    dados
                        .updates_state
                        .channels
                        .push(grammers_session::types::ChannelState { id, pts });
                }
            }
            Ok(())
        })
    }
}

fn chaveiro_erro(e: impl std::fmt::Display) -> Erro {
    Erro::Chaveiro(e.to_string())
}

/// Há sessão guardada no chaveiro (sem ler o valor)?
#[must_use]
pub fn existe_no_chaveiro(chaveiro: &Chaveiro) -> bool {
    chaveiro.tem_segredo_de_mensageiro(NOME_NO_CHAVEIRO)
}

/// Guarda a sessão no chaveiro.
///
/// # Errors
/// [`Erro::Chaveiro`] se ele recusar.
pub fn guardar(chaveiro: &Chaveiro, sessao: &SessaoMtproto) -> Resultado<()> {
    chaveiro
        .guardar_segredo_de_mensageiro(NOME_NO_CHAVEIRO, Segredo::novo(sessao.exportar()?))
        .map_err(chaveiro_erro)
}

/// Lê a sessão do chaveiro; `None` se não há (ainda não entrou).
///
/// # Errors
/// [`Erro::Chaveiro`] se ele recusar; [`Erro::Formato`] se o valor não é uma sessão.
pub fn ler(chaveiro: &Chaveiro) -> Resultado<Option<SessaoMtproto>> {
    if !existe_no_chaveiro(chaveiro) {
        return Ok(None);
    }
    let s = chaveiro
        .segredo_de_mensageiro(NOME_NO_CHAVEIRO)
        .map_err(chaveiro_erro)?;
    SessaoMtproto::restaurar(s.expor()).map(Some)
}

/// Apaga a sessão guardada (sair).
///
/// # Errors
/// [`Erro::Chaveiro`] se ele recusar.
pub fn apagar(chaveiro: &Chaveiro) -> Resultado<()> {
    chaveiro
        .apagar_segredo_de_mensageiro(NOME_NO_CHAVEIRO)
        .map_err(chaveiro_erro)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    fn com_chave(sessao: &SessaoMtproto, dc: i32, byte: u8) {
        let mut d = sessao.dados().unwrap();
        d.home_dc = dc;
        d.dc_options.get_mut(&dc).unwrap().auth_key = Some([byte; 256]);
    }

    #[test]
    fn sessao_nova_nao_tem_chave_e_o_que_se_exporta_volta() {
        let s = SessaoMtproto::default();
        assert!(!s.tem_chave());
        com_chave(&s, 4, 7);
        assert!(s.tem_chave());
        let texto = s.exportar().unwrap();
        let volta = SessaoMtproto::restaurar(&texto).unwrap();
        assert!(volta.tem_chave());
        assert_eq!(volta.home_dc_id().unwrap(), 4);
        assert_eq!(
            volta.dc_option(4).unwrap().unwrap().auth_key,
            Some([7; 256])
        );
        // Os datacenters sem chave não vão no texto (a sessão fica pequena).
        assert!(texto.len() < 2_000);
    }

    #[test]
    fn texto_estranho_e_recusado_sem_vazar_o_conteudo() {
        let e = SessaoMtproto::restaurar("{\"segredo\": \"valor-sigiloso\"").unwrap_err();
        assert!(!e.to_string().contains("valor-sigiloso"));
        let e =
            SessaoMtproto::restaurar("{\"versao\":99,\"home_dc\":1,\"dc_options\":[],\"eu\":[]}")
                .unwrap_err();
        assert!(matches!(e, Erro::Formato(_)));
    }

    #[test]
    fn debug_nao_mostra_nada() {
        let s = SessaoMtproto::default();
        com_chave(&s, 2, 9);
        let d = format!("{s:?}");
        assert!(!d.contains('9') || d.len() < 40);
    }
}
