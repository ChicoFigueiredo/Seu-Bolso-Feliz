//! A coleta: para cada conversa **seguida**, baixa as mídias novas para
//! `<cofre>/entrada/telegram/<conversa>/`, registra cada uma no inventário como achado com
//! origem `telegram:<conversa>:<mensagem>` (achado por sha256: o mesmo arquivo em outra
//! mensagem vira só uma origem a mais) e chama o reconhecimento em cada **arquivo novo**.
//!
//! Checkpoint por conversa (o último id tratado, em [`Estado`]): uma falha numa mídia para só
//! aquela conversa e deixa o checkpoint antes dela; a próxima rodada tenta de novo. Nada aqui
//! envia, apaga ou marca como lida uma mensagem (ADR-017).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use sbf_coleta::areas::extensao_de;
use sbf_coleta::pastas::iso_de;
use sbf_mensageiros::gravador::Gravador;
use sbf_mensageiros::mapeamento::nome_seguro;
use sbf_mensageiros::modelo::{Mensagem, Plataforma};

use crate::config::{Config, pasta_da_conversa};
use crate::estado::Estado;
use crate::fonte::FonteTelegram;
use crate::modelo::{Conversa, Midia, Pedido};
use crate::{Erro, Resultado};

/// A pasta (dentro do cofre) onde as mídias chegam.
pub const PASTA_DE_ENTRADA: &str = "entrada/telegram";

/// A chave de origem: `telegram:<conversa>:<mensagem>`.
#[must_use]
pub fn chave_da_origem(conversa: &str, mensagem: i64) -> String {
    format!("telegram:{conversa}:{mensagem}")
}

/// Quem reconhece o arquivo novo (o `sbf` liga o `ReconhecedorDoCofre`, com as regras de
/// reconhecimento). O erro é texto sem conteúdo do documento.
pub trait Reconhecer {
    /// Reconhece o arquivo em `caminho`.
    ///
    /// # Errors
    /// O motivo, em texto curto.
    fn reconhecer(&mut self, caminho: &Path) -> Result<(), String>;
}

/// Não reconhece nada (só registra o achado).
#[derive(Debug, Clone, Copy, Default)]
pub struct SemReconhecimento;

impl Reconhecer for SemReconhecimento {
    fn reconhecer(&mut self, _: &Path) -> Result<(), String> {
        Ok(())
    }
}

/// O que variar numa coleta.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opcoes {
    /// Só mensagens a partir desta data (segundos unix), e só na primeira coleta da conversa.
    pub desde_data: Option<u64>,
    /// Só esta conversa (id); `None` coleta todas as seguidas.
    pub so_conversa: Option<String>,
}

/// O que uma coleta fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Conversas consultadas.
    pub conversas: usize,
    /// Arquivos novos (conteúdo que o inventário não tinha).
    pub novos: u64,
    /// Mídias cujo conteúdo ou origem o inventário já conhecia.
    pub repetidos: u64,
    /// Arquivos novos que o reconhecimento leu.
    pub reconhecidos: u64,
    /// Arquivos novos que o reconhecimento não conseguiu ler (o achado fica).
    pub falhas_de_reconhecimento: u64,
    /// Conversas que falharam nesta rodada (as outras seguem).
    pub erros: u64,
    /// Parou por cancelamento.
    pub interrompido: bool,
    /// A sessão sumiu ou foi recusada: é preciso `sbf telegram entrar`.
    pub precisa_entrar: bool,
}

fn agora_iso() -> String {
    iso_de(std::time::SystemTime::now())
}

/// Roda uma coleta de `fonte` sobre as conversas seguidas em `config`.
///
/// # Errors
/// Falha do banco ou do disco fora de uma conversa. Falha de uma conversa (rede, mídia
/// expirada) não é erro da coleta: vira [`Relatorio::erros`] e [`Estado::ultimo_erro`].
#[allow(clippy::too_many_arguments)]
pub fn coletar(
    fonte: &mut dyn FonteTelegram,
    conexao: &Connection,
    cofre: &Path,
    config: &Config,
    estado: &mut Estado,
    opcoes: &Opcoes,
    reconhecer: &mut dyn Reconhecer,
    cancelar: &AtomicBool,
) -> Resultado<Relatorio> {
    let mut rel = Relatorio::default();
    let raiz = cofre.join(PASTA_DE_ENTRADA);
    let mut gravador = Gravador::novo(conexao, cofre, Plataforma::Telegram);
    let mut ultimo_erro: Option<String> = None;

    for conversa in config
        .seguidas
        .iter()
        .filter(|c| opcoes.so_conversa.as_deref().is_none_or(|s| s == c.id))
    {
        if cancelar.load(Ordering::Relaxed) {
            rel.interrompido = true;
            break;
        }
        rel.conversas += 1;
        let r = coletar_conversa(
            fonte,
            &mut gravador,
            &raiz,
            conversa,
            config.lote.max(1),
            opcoes,
            estado,
            reconhecer,
            cancelar,
            &mut rel,
        );
        match r {
            Ok(()) => {}
            Err(Erro::Cancelado) => {
                rel.interrompido = true;
                break;
            }
            Err(Erro::PrecisaEntrar) => {
                rel.precisa_entrar = true;
                ultimo_erro = Some(Erro::PrecisaEntrar.to_string());
                break;
            }
            Err(e @ (Erro::Banco(_) | Erro::Io(_))) => {
                gravador.fechar("interrompida")?;
                return Err(e);
            }
            Err(e) => {
                rel.erros += 1;
                ultimo_erro = Some(e.to_string());
            }
        }
    }

    gravador.fechar(if rel.interrompido || rel.precisa_entrar {
        "interrompida"
    } else {
        "concluida"
    })?;
    if rel.conversas > 0 {
        estado.ultima_coleta = Some(agora_iso());
        estado.ultimo_erro = ultimo_erro;
    }
    Ok(rel)
}

#[allow(clippy::too_many_arguments)]
fn coletar_conversa(
    fonte: &mut dyn FonteTelegram,
    gravador: &mut Gravador<'_>,
    raiz: &Path,
    conversa: &Conversa,
    lote: usize,
    opcoes: &Opcoes,
    estado: &mut Estado,
    reconhecer: &mut dyn Reconhecer,
    cancelar: &AtomicBool,
    rel: &mut Relatorio,
) -> Resultado<()> {
    let ponto = estado.ultimo_id(&conversa.id);
    let pedido = Pedido {
        conversa: conversa.id.clone(),
        depois_do_id: ponto,
        desde_data: if ponto == 0 { opcoes.desde_data } else { None },
        limite: lote,
    };
    let pasta = pasta_da_conversa(raiz, conversa);
    let mut ultimo_ok = ponto;
    let mut novos_da_conversa = 0_u64;

    let resultado = fonte.baixar_midias(&pedido, &pasta, &mut |m: Midia| {
        if cancelar.load(Ordering::Relaxed) {
            return Err(Erro::Cancelado);
        }
        let chave = chave_da_origem(&conversa.id, m.mensagem);
        if gravador.ja_tem(&chave)? {
            rel.repetidos += 1;
            // Origem já registrada: o arquivo já tem casa no inventário.
            let _ = std::fs::remove_file(&m.caminho);
            ultimo_ok = ultimo_ok.max(m.mensagem);
            return Ok(());
        }
        let mensagem = Mensagem {
            id: m.mensagem.to_string(),
            conversa: conversa.id.clone(),
            quando: m.quando,
            texto: None,
            anexo: None,
        };
        let nome = nome_seguro(&m.nome);
        let conteudos_antes = gravador.contadores.conteudos_novos;
        gravador.gravar(&mensagem, &chave, &nome, &extensao_de(&nome), &m.caminho)?;
        if gravador.contadores.conteudos_novos > conteudos_antes {
            rel.novos += 1;
            novos_da_conversa += 1;
            match reconhecer.reconhecer(&m.caminho) {
                Ok(()) => rel.reconhecidos += 1,
                Err(_) => rel.falhas_de_reconhecimento += 1,
            }
        } else {
            // Mesmo conteúdo, outra mensagem: só uma origem a mais.
            rel.repetidos += 1;
            let _ = std::fs::remove_file(&m.caminho);
        }
        ultimo_ok = ultimo_ok.max(m.mensagem);
        Ok(())
    });

    let agora = agora_iso();
    let novo_ponto = match &resultado {
        Ok(avanco) => ultimo_ok.max(avanco.ate_id),
        Err(_) => ultimo_ok,
    };
    let e = estado.conversas.entry(conversa.id.clone()).or_default();
    e.ultimo_id = novo_ponto;
    e.arquivos += novos_da_conversa;
    e.ultima_coleta = Some(agora);
    resultado.map(|_| ())
}
