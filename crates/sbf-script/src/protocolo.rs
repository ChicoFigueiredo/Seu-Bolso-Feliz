//! O protocolo do processo filho: uma linha JSON por mensagem.
//!
//! Pedidos: `{"executar": PedidoDeScript}`, `{"verificar": "<fonte>"}` e
//! `{"endurecimento": null}`. Respostas: `{"ok": RespostaDoScript}`, `{"ok": null}`,
//! `{"erro": ErroDoScript}` e `{"endurecimento": RelatorioDeEndurecimento}`.

use std::io::{self, BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};

use sbf_regras::script::{ErroDoScript, ExecutorDeScript, PedidoDeScript, RespostaDoScript};
use serde::{Deserialize, Serialize};

use crate::endurecer::{RelatorioDeEndurecimento, endurecer};
use crate::no_processo::ExecutorNoProcesso;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mensagem {
    Executar(Box<PedidoDeScript>),
    Verificar(String),
    Endurecimento(Option<()>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Resposta {
    Ok(Option<RespostaDoScript>),
    Erro(ErroDoScript),
    Endurecimento(RelatorioDeEndurecimento),
}

/// Atende pedidos até o fim da entrada. Não endurece o processo (ver
/// [`servir_endurecido`]).
///
/// # Errors
/// Só erros de E/S dos canais.
pub fn servir(entrada: impl BufRead, mut saida: impl Write) -> io::Result<()> {
    servir_com(entrada, &mut saida, None)
}

/// Endurece o processo (Landlock e rlimits, melhor esforço) e serve. É a função do
/// subcomando `sbf script --servir` e do binário `sbf-script-filho`.
///
/// # Errors
/// Só erros de E/S dos canais.
pub fn servir_endurecido(entrada: impl BufRead, mut saida: impl Write) -> io::Result<()> {
    let relatorio = endurecer();
    servir_com(entrada, &mut saida, Some(relatorio))
}

fn servir_com(
    entrada: impl BufRead,
    saida: &mut impl Write,
    relatorio: Option<RelatorioDeEndurecimento>,
) -> io::Result<()> {
    let executor = ExecutorNoProcesso::novo();
    for linha in entrada.lines() {
        let linha = linha?;
        if linha.trim().is_empty() {
            continue;
        }
        let resposta = match serde_json::from_str::<Mensagem>(&linha) {
            Err(_) => Resposta::Erro(ErroDoScript::Processo("pedido fora do protocolo".into())),
            Ok(Mensagem::Endurecimento(_)) => match relatorio {
                Some(r) => Resposta::Endurecimento(r),
                None => Resposta::Erro(ErroDoScript::Processo("processo não endurecido".into())),
            },
            Ok(m) => atender(&executor, m),
        };
        let mut bytes = serde_json::to_vec(&resposta)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        bytes.push(b'\n');
        saida.write_all(&bytes)?;
        saida.flush()?;
    }
    Ok(())
}

fn atender(executor: &ExecutorNoProcesso, m: Mensagem) -> Resposta {
    let feito = catch_unwind(AssertUnwindSafe(|| match m {
        Mensagem::Executar(p) => executor.executar(&p).map(Some),
        Mensagem::Verificar(fonte) => executor.verificar(&fonte).map(|()| None),
        Mensagem::Endurecimento(_) => Ok(None),
    }));
    match feito {
        Ok(Ok(r)) => Resposta::Ok(r),
        Ok(Err(e)) => Resposta::Erro(e),
        Err(_) => Resposta::Erro(ErroDoScript::Processo("o executor entrou em pânico".into())),
    }
}
