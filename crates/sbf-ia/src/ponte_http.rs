//! O que `ChatCompletions` e `Anthropic` partilham: abrir o portão de rede para um endereço
//! escolhido pelo usuário e esperar a resposta sem prender o botão Parar.
//!
//! O portão só deixa passar HTTPS do catálogo. Um endereço `http://` só vale em loopback
//! (Ollama em `http://localhost:11434`): entra no catálogo como `https://` do mesmo host e
//! porta e é roteado para o `http://` de loopback, que o portão aceita. Qualquer outro
//! `http://` é recusado antes de abrir conexão.

use crate::{ErroDeIa, ResultadoDeIa};
use bancada_rede::{
    Autorizacao, Catalogo, ConfiguracaoDaRede, Endereco, ErroDeRede, FonteDeAutorizacao, Pedido,
    Rede, Resposta,
};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::openrouter::FonteDaChave;

pub(crate) struct Autorizador(pub Arc<dyn FonteDaChave>);

impl fmt::Debug for Autorizador {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Autorizador(***)")
    }
}

impl FonteDeAutorizacao for Autorizador {
    fn autorizacao(&self, _endereco: &Endereco) -> Option<Autorizacao> {
        self.0.chave().and_then(|c| Autorizacao::portador(&c))
    }
}

/// O endereço depois de lido: a base HTTPS do catálogo e, em loopback, a rota para o HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnderecoLido {
    /// `https://host[:porta]/caminho`, sem barra no fim.
    pub base: String,
    /// `host[:porta]` e `http://host[:porta]` quando o endereço dado era HTTP em loopback.
    pub rota: Option<(String, String)>,
}

fn eh_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// Lê o endereço dado pelo usuário. Só `https://` ou `http://` em loopback.
pub(crate) fn ler_endereco(dado: &str) -> ResultadoDeIa<EnderecoLido> {
    let dado = dado.trim().trim_end_matches('/');
    if let Some(resto) = dado.strip_prefix("https://") {
        if resto.is_empty() || resto.contains(['?', '#', ' ']) {
            return Err(ErroDeIa::Indisponivel(
                "endereço do provedor inválido".to_owned(),
            ));
        }
        return Ok(EnderecoLido {
            base: dado.to_owned(),
            rota: None,
        });
    }
    if let Some(resto) = dado.strip_prefix("http://") {
        let autoridade = resto.split('/').next().unwrap_or("");
        let host = autoridade
            .rsplit_once(':')
            .filter(|(_, porta)| porta.bytes().all(|b| b.is_ascii_digit()))
            .map_or(autoridade, |(h, _)| h);
        if !eh_loopback(host) || resto.contains(['?', '#', ' ']) {
            return Err(ErroDeIa::Indisponivel(
                "o endereço precisa ser https (http só em localhost)".to_owned(),
            ));
        }
        let caminho = &resto[autoridade.len()..];
        return Ok(EnderecoLido {
            base: format!("https://{autoridade}{caminho}"),
            rota: Some((
                autoridade.to_ascii_lowercase(),
                format!("http://{autoridade}"),
            )),
        });
    }
    Err(ErroDeIa::Indisponivel(
        "o endereço precisa começar com https:// (ou http://localhost)".to_owned(),
    ))
}

/// Confere um endereço dado pelo usuário com a mesma leitura estrita que o provedor usa para
/// abrir a conexão: `https://`, ou `http://` só em loopback (host exatamente `localhost`,
/// `127.0.0.1` ou `[::1]`). Quem valida a configuração chama esta função, para não aceitar
/// um endereço que o provedor depois recusaria (ou, pior, que um prefixo enganoso deixaria
/// passar, como `http://localhost.exemplo.com`).
///
/// # Errors
/// Texto para a tela, sem o endereço.
pub fn validar_endereco(dado: &str) -> Result<(), String> {
    ler_endereco(dado).map(|_| ()).map_err(|e| match e {
        ErroDeIa::Indisponivel(m) => m,
        outro => outro.to_string(),
    })
}

/// Abre o portão para um provedor. `com_chave`: o pedido leva `Authorization: Bearer` da fonte.
pub(crate) fn abrir_rede(
    nome: &str,
    endereco: &EnderecoLido,
    fonte: Option<Arc<dyn FonteDaChave>>,
    prazo: Duration,
) -> ResultadoDeIa<Rede> {
    let mut ponto = Endereco::novo(nome, &endereco.base)
        .map_err(|_| ErroDeIa::Indisponivel("endereço do provedor inválido".to_owned()))?;
    if fonte.is_some() {
        ponto = ponto.com_autorizacao();
    }
    let mut config = ConfiguracaoDaRede::nova(Catalogo::novo().com(ponto))
        .prazo(prazo)
        .agente_de_usuario("sbf-ia");
    if let Some(f) = fonte {
        config = config.autorizacao(Arc::new(Autorizador(f)));
    }
    if let Some((autoridade, http)) = &endereco.rota {
        config = config.rota(autoridade, http);
    }
    Rede::abrir(config).map_err(|e| ErroDeIa::Indisponivel(format!("portão de rede: {e}")))
}

/// Executa o pedido e espera, olhando o botão Parar a cada 25 ms.
pub(crate) fn executar(
    rede: &Rede,
    pedido: Pedido,
    cancelar: &AtomicBool,
) -> Result<Resposta, ErroDeRede> {
    let mut pendente = rede.buscar(pedido);
    loop {
        if let Some(r) = pendente.tentar_pegar() {
            return r;
        }
        if cancelar.load(Ordering::Acquire) {
            pendente.cancelar();
            return Err(ErroDeRede::Cancelado);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Erro de rede para erro de IA, sem corpo nem cabeçalho (podem ecoar o pedido).
pub(crate) fn mapear(rede: &Rede, nome: &str, erro: &ErroDeRede) -> ErroDeIa {
    match erro {
        ErroDeRede::Cancelado => ErroDeIa::Interrompido,
        ErroDeRede::Limitado { ate } => {
            let agora = rede.relogio().agora();
            ErroDeIa::Limite {
                espera_segundos: ate.desde(agora).as_secs().max(1),
            }
        }
        ErroDeRede::Http { status: 402, .. } => ErroDeIa::Teto,
        ErroDeRede::AutorizacaoRecusada { .. } | ErroDeRede::SemCredencial { .. } => {
            ErroDeIa::Indisponivel(format!("chave de {nome} ausente ou recusada"))
        }
        ErroDeRede::Http { status, .. } => ErroDeIa::Outro(format!("{nome}: HTTP {status}")),
        outro if outro.e_falta_de_rede() => ErroDeIa::Outro(format!("{nome} inacessível")),
        _ => ErroDeIa::Outro(format!("falha ao falar com {nome}")),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn https_vale_como_esta() {
        let e = ler_endereco("https://openrouter.ai/api/v1/").expect("endereço");
        assert_eq!(e.base, "https://openrouter.ai/api/v1");
        assert!(e.rota.is_none());
    }

    #[test]
    fn http_em_loopback_vira_rota() {
        let e = ler_endereco("http://localhost:11434/v1").expect("endereço");
        assert_eq!(e.base, "https://localhost:11434/v1");
        assert_eq!(
            e.rota,
            Some((
                "localhost:11434".to_owned(),
                "http://localhost:11434".to_owned()
            ))
        );
        let e = ler_endereco("http://127.0.0.1:8080").expect("endereço");
        assert_eq!(e.base, "https://127.0.0.1:8080");
    }

    #[test]
    fn http_fora_de_loopback_e_recusado() {
        for ruim in [
            "http://exemplo.com/v1",
            "http://192.168.0.5:11434/v1",
            "ftp://x",
            "localhost:11434",
            "http://localhost.exemplo.com/v1",
            "https://",
        ] {
            assert!(ler_endereco(ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn validar_endereco_e_a_mesma_leitura_estrita_do_provedor() {
        for bom in [
            "https://api.exemplo.com/v1",
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(validar_endereco(bom).is_ok(), "{bom}");
        }
        for ruim in [
            "http://localhost.evil.com",
            "http://localhost.evil.com/v1",
            "http://localhost@evil.com/v1",
            "http://localhost:11434@evil.com/v1",
            "http://127.0.0.1.evil.com",
            "http://evil.com/localhost",
            "http://localhostevil.com",
            "",
        ] {
            assert!(validar_endereco(ruim).is_err(), "{ruim}");
        }
    }
}
