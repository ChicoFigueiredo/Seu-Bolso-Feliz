//! A coleta de uma conta de e-mail: os rótulos que a pessoa segue, um de cada vez, cada um com
//! o seu checkpoint, mais os termos extras da conta.
//!
//! - **`--consulta` (ou `consulta` do MCP)** é um teste avulso: uma coleta só, sem rótulo, com
//!   essa consulta; nunca vira a consulta da conta.
//! - **Rótulos seguidos** (`email.json`): para cada um, `has:attachment` + os termos extras, na
//!   janela do checkpoint do rótulo (na primeira vez, 90 dias).
//! - **Nada escolhido**: o rótulo `Comprovantes` se a conta o tem; senão a consulta antiga
//!   (`has:attachment newer_than:30d -category:promotions`) na caixa inteira.
//! - **Nada seguido de propósito** (lista vazia): nada é coletado.
//!
//! Falha num rótulo não impede os outros (a falha vira contagem), mas autorização vencida e
//! cancelamento interrompem tudo.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::Chaveiro;

use crate::coletor::{OpcoesDaColeta, ResumoDaColeta, coletar};
use crate::config::{ARQUIVO_DA_CONFIG, Config, Conta, Seguidos, guardar_rotulos_da_conta};
use crate::credenciais::{Alvo, presentes};
use crate::erro::{Erro, Resultado};
use crate::fonte::FonteDeGmail;
use crate::reconhecimento::Reconhecimento;

/// Coleta a `conta`. `arquivo_dos_rotulos`, se dado, recebe a lista de rótulos da conta (para
/// a janela escolher sem rede).
///
/// # Errors
/// [`Erro::Autorizacao`], [`Erro::Cancelado`], [`Erro::Argumento`], e o primeiro erro quando
/// **nenhum** rótulo pôde ser coletado.
pub fn coletar_conta(
    cofre: &Path,
    conta: &Conta,
    fonte: &dyn FonteDeGmail,
    reconhecimento: &dyn Reconhecimento,
    opcoes: &OpcoesDaColeta,
    arquivo_dos_rotulos: Option<&Path>,
    cancelar: &AtomicBool,
) -> Resultado<ResumoDaColeta> {
    let base = OpcoesDaColeta {
        conta: Some(conta.id.clone()),
        rotulo: None,
        ..opcoes.clone()
    };
    if opcoes.consulta.is_some() {
        return coletar(cofre, fonte, reconhecimento, &base, cancelar);
    }
    let da_conta = fonte.rotulos()?;
    if let Some(a) = arquivo_dos_rotulos {
        let _ = guardar_rotulos_da_conta(a, &da_conta);
    }
    let nomes: Vec<String> = da_conta.iter().map(|r| r.nome.clone()).collect();
    let alvos = match conta.seguidos(&nomes) {
        Seguidos::CaixaInteira => return coletar(cofre, fonte, reconhecimento, &base, cancelar),
        Seguidos::Rotulos(v) => v,
    };
    let extra = Some(conta.consulta_extra.trim().to_owned()).filter(|e| !e.is_empty());
    let mut total = ResumoDaColeta::default();
    let mut primeiro_erro: Option<Erro> = None;
    let mut coletados = 0_usize;
    for nome in &alvos {
        let Some(r) = da_conta
            .iter()
            .find(|r| r.nome.trim().to_lowercase() == nome.trim().to_lowercase())
        else {
            total.rotulos_ausentes += 1;
            continue;
        };
        let o = OpcoesDaColeta {
            consulta: extra.clone(),
            rotulo: Some(r.clone()),
            ..base.clone()
        };
        match coletar(cofre, fonte, reconhecimento, &o, cancelar) {
            Ok(r) => {
                coletados += 1;
                total.somar(&r);
            }
            Err(e @ (Erro::Autorizacao | Erro::Cancelado | Erro::Argumento(_))) => return Err(e),
            Err(e) => {
                total.falhas += 1;
                primeiro_erro.get_or_insert(e);
            }
        }
    }
    match primeiro_erro {
        Some(e) if coletados == 0 => Err(e),
        _ => Ok(total),
    }
}

/// As contas de e-mail: as de `email.json` (na pasta de dados do app) ou, sem nenhuma gravada,
/// a conta legada `gmail` se há credenciais antigas (chaveiro, ambiente ou `.env`).
///
/// # Errors
/// [`Erro::Argumento`] se `email.json` não é válido (JSON ruim, id de conta inválido ou
/// repetido): nada de conta é usada com um arquivo recusado.
pub fn contas_efetivas(dados: Option<&Path>, chaveiro: Option<&Chaveiro>) -> Resultado<Config> {
    let legado = presentes(chaveiro, Alvo::legado());
    match dados {
        Some(d) => Config::carregar_efetiva(&d.join(ARQUIVO_DA_CONFIG), legado),
        None => {
            let mut c = Config::default();
            if legado {
                c.contas.push(Conta::legada());
            }
            Ok(c)
        }
    }
}
