//! O executor que o app usa: um processo filho endurecido, um por lote.
//!
//! O pai só confia no próprio relógio: se o filho passar do prazo (`tempo_ms` do pedido mais
//! uma margem), é morto e o pedido devolve [`ErroDoScript::Tempo`]; o pedido seguinte sobe um
//! filho novo. O mesmo vale para qualquer erro de limite e para filho que morre ou responde
//! fora do protocolo ([`ErroDoScript::Processo`]).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use sbf_regras::script::{
    ErroDoScript, ExecutorDeScript, Limites, PedidoDeScript, RespostaDoScript,
};
use tempfile::TempDir;

use crate::endurecer::RelatorioDeEndurecimento;
use crate::protocolo::{Mensagem, Resposta};

/// Folga sobre o `tempo_ms` do pedido (a VM já corta no prazo; isto só pega o que a VM não
/// consegue interromper).
const MARGEM: Duration = Duration::from_millis(1000);
/// Folga a mais na primeira conversa com um filho recém-subido.
const MARGEM_DE_SUBIDA: Duration = Duration::from_millis(2000);
/// Teto de uma linha de resposta lida do filho.
const LINHA_MAXIMA: usize = 16 * 1024 * 1024;

#[derive(Debug)]
struct Filho {
    processo: Child,
    envio: Sender<Vec<u8>>,
    respostas: Receiver<Option<Vec<u8>>>,
    recem_subido: bool,
    // Mantém o diretório de trabalho vazio vivo enquanto o filho vive.
    _pasta: TempDir,
}

impl Drop for Filho {
    fn drop(&mut self) {
        let _ = self.processo.kill();
        let _ = self.processo.wait();
    }
}

/// Executa scripts num processo filho.
#[derive(Debug)]
pub struct ExecutorFilho {
    programa: PathBuf,
    argumentos: Vec<String>,
    filho: Mutex<Option<Filho>>,
}

impl ExecutorFilho {
    /// `programa` + `argumentos` são o que sobe o filho (o binário `sbf-script-filho`, ou
    /// `sbf script --servir`). Nada sobe até o primeiro pedido.
    #[must_use]
    pub fn novo(programa: PathBuf, argumentos: Vec<String>) -> Self {
        Self {
            programa,
            argumentos,
            filho: Mutex::new(None),
        }
    }

    /// Pergunta ao filho (subindo-o, se preciso) o que o endurecimento de SO conseguiu.
    ///
    /// # Errors
    /// [`ErroDoScript::Processo`] se o filho não responder.
    pub fn endurecimento(&self) -> Result<RelatorioDeEndurecimento, ErroDoScript> {
        let limites = Limites::default();
        match self.pedir(&Mensagem::Endurecimento(None), &limites, 100)? {
            Resposta::Endurecimento(r) => Ok(r),
            Resposta::Erro(e) => Err(e),
            Resposta::Ok(_) => Err(ErroDoScript::Processo("resposta fora do protocolo".into())),
        }
    }

    fn subir(&self) -> Result<Filho, ErroDoScript> {
        let falha = |e: io::Error| ErroDoScript::Processo(format!("não subiu: {}", e.kind()));
        let pasta = tempfile::tempdir().map_err(falha)?;
        let mut processo = Command::new(&self.programa)
            .args(&self.argumentos)
            .env_clear()
            .current_dir(pasta.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(falha)?;
        let (Some(stdin), Some(stdout)) = (processo.stdin.take(), processo.stdout.take()) else {
            let _ = processo.kill();
            return Err(ErroDoScript::Processo("sem canais com o filho".into()));
        };
        let (envio, a_enviar) = mpsc::channel::<Vec<u8>>();
        let (para_o_pai, respostas) = mpsc::channel::<Option<Vec<u8>>>();
        thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(linha) = a_enviar.recv() {
                if stdin
                    .write_all(&linha)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        thread::spawn(move || {
            let mut leitor = BufReader::new(stdout);
            loop {
                match ler_linha(&mut leitor) {
                    Ok(Some(l)) => {
                        if para_o_pai.send(Some(l)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = para_o_pai.send(None);
                        break;
                    }
                }
            }
        });
        Ok(Filho {
            processo,
            envio,
            respostas,
            recem_subido: true,
            _pasta: pasta,
        })
    }

    fn pedir(
        &self,
        mensagem: &Mensagem,
        limites: &Limites,
        tempo_ms: u64,
    ) -> Result<Resposta, ErroDoScript> {
        let mut linha = serde_json::to_vec(mensagem)
            .map_err(|e| ErroDoScript::Processo(format!("não serializou o pedido: {e}")))?;
        if linha.len() > limites.entrada_bytes.saturating_add(1024) {
            return Err(ErroDoScript::Grande(format!(
                "pedido de {} bytes passa de {}",
                linha.len(),
                limites.entrada_bytes
            )));
        }
        linha.push(b'\n');
        let mut guarda = self
            .filho
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guarda.is_none() {
            *guarda = Some(self.subir()?);
        }
        let Some(filho) = guarda.as_mut() else {
            return Err(ErroDoScript::Processo("filho ausente".into()));
        };
        let mut prazo = Duration::from_millis(tempo_ms) + MARGEM;
        if filho.recem_subido {
            prazo += MARGEM_DE_SUBIDA;
            filho.recem_subido = false;
        }
        let resultado = if filho.envio.send(linha).is_err() {
            Err(ErroDoScript::Processo("o filho fechou a entrada".into()))
        } else {
            match filho.respostas.recv_timeout(prazo) {
                Ok(Some(bytes)) => interpretar(&bytes, limites),
                Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                    Err(ErroDoScript::Processo("o filho morreu".into()))
                }
                Err(RecvTimeoutError::Timeout) => Err(ErroDoScript::Tempo),
            }
        };
        // Depois de qualquer erro de limite ou de processo, um filho novo no próximo pedido.
        if matches!(
            resultado,
            Err(ErroDoScript::Tempo
                | ErroDoScript::Memoria
                | ErroDoScript::Instrucoes
                | ErroDoScript::Processo(_))
        ) || matches!(
            &resultado,
            Ok(Resposta::Erro(
                ErroDoScript::Tempo | ErroDoScript::Memoria | ErroDoScript::Instrucoes
            ))
        ) {
            *guarda = None;
        }
        resultado
    }
}

fn interpretar(bytes: &[u8], limites: &Limites) -> Result<Resposta, ErroDoScript> {
    if bytes.len() > limites.saida_bytes.saturating_add(4096) {
        return Err(ErroDoScript::Grande(format!(
            "resposta de {} bytes passa de {}",
            bytes.len(),
            limites.saida_bytes
        )));
    }
    serde_json::from_slice::<Resposta>(bytes)
        .map_err(|_| ErroDoScript::Processo("resposta fora do protocolo".into()))
}

fn ler_linha(leitor: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let lidos = leitor
        .by_ref()
        .take(LINHA_MAXIMA as u64 + 1)
        .read_until(b'\n', &mut buf)?;
    if lidos == 0 {
        return Ok(None);
    }
    if buf.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "linha sem fim ou grande demais",
        ));
    }
    buf.pop();
    Ok(Some(buf))
}

impl ExecutorDeScript for ExecutorFilho {
    fn executar(&self, pedido: &PedidoDeScript) -> Result<RespostaDoScript, ErroDoScript> {
        let m = Mensagem::Executar(Box::new(pedido.clone()));
        match self.pedir(&m, &pedido.limites, pedido.limites.tempo_ms)? {
            Resposta::Ok(Some(r)) => Ok(r),
            Resposta::Erro(e) => Err(e),
            _ => Err(ErroDoScript::Processo("resposta fora do protocolo".into())),
        }
    }

    fn verificar(&self, fonte: &str) -> Result<(), ErroDoScript> {
        let limites = Limites::default();
        match self.pedir(&Mensagem::Verificar(fonte.to_owned()), &limites, 100)? {
            Resposta::Ok(None) => Ok(()),
            Resposta::Erro(e) => Err(e),
            _ => Err(ErroDoScript::Processo("resposta fora do protocolo".into())),
        }
    }
}

// Garante, em tempo de compilação, que o executor atravessa threads.
const _: fn() = || {
    fn exige<T: Send + Sync>() {}
    exige::<ExecutorFilho>();
};
