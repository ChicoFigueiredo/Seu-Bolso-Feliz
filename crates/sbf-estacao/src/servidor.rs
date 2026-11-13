//! O servidor com que a estação fala e o cliente HTTP do contrato (`sbf-contrato`) sobre o
//! portão da `bancada-rede`. Só HTTPS, só o endereço do catálogo.

use bancada_rede::{Endereco, ErroDeRede, Pedido, Rede, Resposta};
use sbf_contrato::{
    CodigoDeErro, Colecao, ErroDaApi, Estacao, Estacoes, Estado, Id, LinhasDaColecao, Pareamento,
    PedidoDePareamento, PropostaDeAlteracao, RespostaDaAlteracao, VERSAO_DO_CONTRATO, prefixo,
};
use serde::de::DeserializeOwned;
use std::fmt;
use std::time::Duration;

/// Prazo de uma coleção inteira (a de movimentações é a maior).
const PRAZO_DA_COLECAO: Duration = Duration::from_secs(60);

/// Onde está o servidor: `https://host[:porta]`, sem caminho. O host é também a chave da
/// credencial no chaveiro (`estacao:<host>`) e o nome da pasta do cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Servidor {
    base: String,
    host: String,
    versao_do_contrato: u32,
}

impl Servidor {
    /// # Errors
    /// Endereço que não é `https://host[:porta]` simples.
    pub fn novo(base: &str) -> Result<Self, ErroDeRede> {
        let base = base.trim_end_matches('/');
        let endereco = Endereco::novo("servidor", &format!("{base}/api"))?;
        if endereco.prefixo() != "/api" {
            return Err(ErroDeRede::EnderecoForaDoCatalogo {
                endereco: base.to_owned(),
            });
        }
        Ok(Self {
            base: base.to_owned(),
            host: endereco.autoridade(),
            versao_do_contrato: VERSAO_DO_CONTRATO,
        })
    }

    /// Outra versão de contrato (testes da regra Q4).
    pub fn com_versao_do_contrato(mut self, versao: u32) -> Self {
        self.versao_do_contrato = versao;
        self
    }

    /// `host[:porta]`.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn versao_do_contrato(&self) -> u32 {
        self.versao_do_contrato
    }

    /// O endereço do catálogo: `https://host/api`, o único que recebe a credencial.
    ///
    /// # Errors
    /// Nunca, para um `Servidor` já validado.
    pub fn endereco(&self) -> Result<Endereco, ErroDeRede> {
        Ok(Endereco::novo("servidor", &format!("{}/api", self.base))?.com_autorizacao())
    }

    fn url(&self, recurso: &str) -> String {
        format!("{}{}{recurso}", self.base, prefixo(self.versao_do_contrato))
    }
}

/// O que deu errado ao falar com o servidor, já com o sentido do contrato.
#[derive(Debug, Clone, PartialEq)]
pub enum ErroDoServidor {
    /// O servidor não foi alcançado: mostrar a cópia.
    SemRede(ErroDeRede),
    /// `401 estacao_revogada` (grill Q11).
    EstacaoRevogada,
    /// `401` sem detalhe: a credencial não vale (ou não há credencial).
    NaoAutenticado,
    /// `426 contrato_antigo`: atualize o app.
    ContratoAntigo,
    /// `426 contrato_mais_novo`: o servidor ainda não foi atualizado.
    ContratoMaisNovo,
    /// Outro erro da API, com o código e a mensagem do servidor.
    Api {
        status: u16,
        codigo: Option<CodigoDeErro>,
        mensagem: String,
    },
    /// A resposta não segue o contrato.
    RespostaMalformada(String),
    /// Falha local do portão (fora do catálogo, cache…).
    Rede(ErroDeRede),
}

impl fmt::Display for ErroDoServidor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemRede(e) => write!(f, "sem rede: {e}"),
            Self::EstacaoRevogada => f.write_str("esta estação foi revogada"),
            Self::NaoAutenticado => {
                f.write_str("o servidor não reconhece esta estação; faça o pareamento de novo")
            }
            Self::ContratoAntigo => f.write_str("o servidor pede um app mais novo: atualize o app"),
            Self::ContratoMaisNovo => f.write_str(
                "este app é mais novo que o servidor: o servidor precisa ser atualizado",
            ),
            Self::Api {
                status, mensagem, ..
            } => write!(f, "o servidor respondeu HTTP {status}: {mensagem}"),
            Self::RespostaMalformada(o_que) => {
                write!(f, "a resposta do servidor não segue o contrato ({o_que})")
            }
            Self::Rede(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ErroDoServidor {}

fn traduzir(erro: ErroDeRede) -> ErroDoServidor {
    if erro.e_falta_de_rede() {
        return ErroDoServidor::SemRede(erro);
    }
    if matches!(erro, ErroDeRede::SemCredencial { .. }) {
        return ErroDoServidor::NaoAutenticado;
    }
    let (Some(status), Some(corpo)) = (erro.status_http(), erro.corpo()) else {
        return match erro.status_http() {
            Some(status) => ErroDoServidor::Api {
                status,
                codigo: None,
                mensagem: erro.to_string(),
            },
            None => ErroDoServidor::Rede(erro),
        };
    };
    let api: Option<ErroDaApi> = serde_json::from_slice(corpo.bytes()).ok();
    match api.as_ref().map(|a| a.codigo) {
        Some(CodigoDeErro::EstacaoRevogada) => ErroDoServidor::EstacaoRevogada,
        Some(CodigoDeErro::NaoAutenticado) => ErroDoServidor::NaoAutenticado,
        Some(CodigoDeErro::ContratoAntigo) => ErroDoServidor::ContratoAntigo,
        Some(CodigoDeErro::ContratoMaisNovo) => ErroDoServidor::ContratoMaisNovo,
        _ if status == 401 => ErroDoServidor::NaoAutenticado,
        codigo => ErroDoServidor::Api {
            status,
            codigo,
            mensagem: api.map_or_else(|| erro.to_string(), |a| a.mensagem),
        },
    }
}

/// Uma coleção que mudou: as linhas e a `ETag` nova.
#[derive(Debug, Clone, PartialEq)]
pub struct ColecaoNova {
    pub linhas: LinhasDaColecao,
    pub etag: Option<String>,
}

/// O cliente do contrato. Bloqueia (roda na thread da sincronização ou do pareamento).
#[derive(Debug, Clone)]
pub struct ClienteDoServidor {
    rede: Rede,
    servidor: Servidor,
}

fn ler_json<T: DeserializeOwned>(resposta: &Resposta, o_que: &str) -> Result<T, ErroDoServidor> {
    serde_json::from_slice(&resposta.corpo)
        .map_err(|_| ErroDoServidor::RespostaMalformada(o_que.to_owned()))
}

impl ClienteDoServidor {
    pub fn novo(rede: Rede, servidor: Servidor) -> Self {
        Self { rede, servidor }
    }

    pub fn servidor(&self) -> &Servidor {
        &self.servidor
    }

    pub fn rede(&self) -> &Rede {
        &self.rede
    }

    fn executar(&self, pedido: Pedido) -> Result<Resposta, ErroDoServidor> {
        self.rede
            .executar(&pedido.aceitar("application/json"))
            .map_err(traduzir)
    }

    /// `GET /estado`.
    ///
    /// # Errors
    /// [`ErroDoServidor`].
    pub fn estado(&self) -> Result<Estado, ErroDoServidor> {
        let r = self.executar(Pedido::get(self.servidor.url("/estado")).autorizado())?;
        ler_json(&r, "estado")
    }

    /// `GET /livro/{colecao}` com a `ETag` guardada. `None`: não mudou (`304`).
    ///
    /// # Errors
    /// [`ErroDoServidor`].
    pub fn colecao(
        &self,
        colecao: Colecao,
        etag: Option<&str>,
    ) -> Result<Option<ColecaoNova>, ErroDoServidor> {
        let mut pedido = Pedido::get(self.servidor.url(&format!("/livro/{}", colecao.caminho())))
            .autorizado()
            .prazo(PRAZO_DA_COLECAO);
        if let Some(etag) = etag {
            pedido = pedido.se_mudou_desde(etag);
        }
        let r = self.executar(pedido)?;
        if r.status == 304 {
            return Ok(None);
        }
        let linhas: LinhasDaColecao = ler_json(&r, colecao.caminho())?;
        if linhas.colecao() != colecao {
            return Err(ErroDoServidor::RespostaMalformada(format!(
                "pedi {} e veio {}",
                colecao.caminho(),
                linhas.colecao().caminho()
            )));
        }
        Ok(Some(ColecaoNova {
            linhas,
            etag: r.etag,
        }))
    }

    /// `POST /estacoes/pareamento` (sem credencial).
    ///
    /// # Errors
    /// [`ErroDoServidor`] (`Api` com [`CodigoDeErro::PareamentoRecusado`] quando o login não
    /// é aceito).
    pub fn parear(&self, pedido: &PedidoDePareamento) -> Result<Pareamento, ErroDoServidor> {
        let corpo = serde_json::to_vec(pedido)
            .map_err(|_| ErroDoServidor::RespostaMalformada("pedido de pareamento".to_owned()))?;
        let r =
            self.executar(Pedido::post(self.servidor.url("/estacoes/pareamento")).json(corpo))?;
        let corpo = zeroize::Zeroizing::new(r.corpo);
        serde_json::from_slice(&corpo)
            .map_err(|_| ErroDoServidor::RespostaMalformada("pareamento".to_owned()))
    }

    /// `GET /estacoes`.
    ///
    /// # Errors
    /// [`ErroDoServidor`].
    pub fn estacoes(&self) -> Result<Estacoes, ErroDoServidor> {
        let r = self.executar(Pedido::get(self.servidor.url("/estacoes")).autorizado())?;
        ler_json(&r, "estacoes")
    }

    /// `POST /estacoes/{id}/revogar`.
    ///
    /// # Errors
    /// [`ErroDoServidor`].
    pub fn revogar(&self, estacao: Id) -> Result<Estacao, ErroDoServidor> {
        let r = self.executar(
            Pedido::post(self.servidor.url(&format!("/estacoes/{estacao}/revogar"))).autorizado(),
        )?;
        ler_json(&r, "estacao revogada")
    }

    /// `POST /alteracoes`.
    ///
    /// # Errors
    /// [`ErroDoServidor`] (a recusa **não** é erro: vem em [`RespostaDaAlteracao::Recusada`]).
    pub fn propor(
        &self,
        proposta: &PropostaDeAlteracao,
    ) -> Result<RespostaDaAlteracao, ErroDoServidor> {
        let corpo = serde_json::to_vec(proposta)
            .map_err(|_| ErroDoServidor::RespostaMalformada("proposta".to_owned()))?;
        let r = self.executar(
            Pedido::post(self.servidor.url("/alteracoes"))
                .autorizado()
                .json(corpo),
        )?;
        ler_json(&r, "resposta da alteracao")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_https_sem_caminho() {
        let s = Servidor::novo("https://sbf.exemplo.com.br/").expect("servidor");
        assert_eq!(s.host(), "sbf.exemplo.com.br");
        assert_eq!(s.url("/estado"), "https://sbf.exemplo.com.br/api/v1/estado");
        assert!(Servidor::novo("http://sbf.exemplo.com.br").is_err());
        assert!(Servidor::novo("https://sbf.exemplo.com.br/x").is_err());
        assert_eq!(
            Servidor::novo("https://sbf.exemplo.com.br:8443")
                .expect("porta")
                .host(),
            "sbf.exemplo.com.br:8443"
        );
    }
}
