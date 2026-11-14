//! A ponte do WhatsApp: o processo externo que fala o protocolo do WhatsApp, e o lado do SBF
//! que o conduz. Só leitura (ver [`super::protocolo`]).
//!
//! - [`Canal`]: linhas de ida e volta. [`ProcessoExterno`] é o canal de verdade (um filho com
//!   stdin e stdout em pipe, ambiente mínimo, stderr descartado); os testes usam um falso.
//! - [`PonteDoWhatsapp`]: a [`FonteDeMensagens`] sobre um canal.
//! - [`Pareamento`]: o fluxo do QR, para a janela.
//!
//! A sessão chega da ponte como evento `sessao` e é guardada cifrada na hora
//! ([`super::sessao`]); ao subir, a ponte recebe a sessão decifrada, só pelo pipe.

use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::protocolo::{Comando, Evento, Linha, Resposta, ler_linha};
use super::sessao::SessaoCifrada;
use crate::modelo::{Checkpoint, Conversa, Mensagem, Plataforma};
use crate::{Erro, FonteDeMensagens, Resultado};

/// Quanto esperar a resposta de um comando comum.
pub const PRAZO_DO_COMANDO: Duration = Duration::from_secs(120);
/// Quanto esperar um download de mídia.
pub const PRAZO_DO_DOWNLOAD: Duration = Duration::from_secs(300);

/// Linhas de ida e volta com a ponte.
pub trait Canal {
    /// Manda uma linha (sem o `\n`).
    ///
    /// # Errors
    /// A ponte morreu.
    fn enviar(&mut self, linha: &str) -> Resultado<()>;
    /// Espera uma linha até `prazo`. `Ok(None)`: nada chegou a tempo.
    ///
    /// # Errors
    /// A ponte morreu e não há mais linhas.
    fn receber(&mut self, prazo: Duration) -> Resultado<Option<String>>;
    /// Encerra a ponte.
    fn encerrar(&mut self);
}

/// O canal de verdade: um processo filho.
#[derive(Debug)]
pub struct ProcessoExterno {
    filho: Child,
    entrada: Option<ChildStdin>,
    linhas: Receiver<String>,
}

impl ProcessoExterno {
    /// Sobe `programa` (a ponte), sem argumentos secretos: a sessão vai pelo pipe.
    ///
    /// # Errors
    /// O programa não existe ou não executa.
    pub fn subir(programa: &Path) -> Resultado<Self> {
        let mut cmd = Command::new(programa);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            // Ambiente mínimo: nada do ambiente do SBF (chaves, DATABASE_URL...) vaza.
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin");
        if let Some(home) = std::env::var_os("HOME") {
            cmd.env("HOME", home);
        }
        let mut filho = cmd
            .spawn()
            .map_err(|_| Erro::Fonte("a ponte do WhatsApp não executa".into()))?;
        let entrada = filho.stdin.take();
        let saida = filho
            .stdout
            .take()
            .ok_or_else(|| Erro::Fonte("a ponte não tem saída".into()))?;
        let (tx, linhas) = channel();
        std::thread::spawn(move || {
            for l in BufReader::new(saida).lines() {
                let Ok(l) = l else { break };
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            filho,
            entrada,
            linhas,
        })
    }
}

impl Canal for ProcessoExterno {
    fn enviar(&mut self, linha: &str) -> Resultado<()> {
        let e = self
            .entrada
            .as_mut()
            .ok_or_else(|| Erro::Fonte("a ponte foi encerrada".into()))?;
        e.write_all(linha.as_bytes())
            .and_then(|()| e.write_all(b"\n"))
            .and_then(|()| e.flush())
            .map_err(|_| Erro::Fonte("a ponte não responde".into()))
    }

    fn receber(&mut self, prazo: Duration) -> Resultado<Option<String>> {
        match self.linhas.recv_timeout(prazo) {
            Ok(l) => Ok(Some(l)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Erro::Fonte("a ponte terminou".into())),
        }
    }

    fn encerrar(&mut self) {
        self.entrada = None; // fecha o stdin: a ponte sai sozinha
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

impl Drop for ProcessoExterno {
    fn drop(&mut self) {
        self.encerrar();
    }
}

/// A [`FonteDeMensagens`] do WhatsApp sobre um [`Canal`].
pub struct PonteDoWhatsapp<C: Canal> {
    canal: C,
    sessao: SessaoCifrada,
    proximo_id: u64,
    iniciada: bool,
}

impl<C: Canal> std::fmt::Debug for PonteDoWhatsapp<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PonteDoWhatsapp")
            .field("iniciada", &self.iniciada)
            .finish_non_exhaustive()
    }
}

/// Trata um evento fora de hora: sessão nova é guardada; queda vira erro.
fn tratar_evento(e: Evento, sessao: &SessaoCifrada) -> Resultado<()> {
    match e {
        Evento::Sessao { blob } => {
            let bruto = STANDARD
                .decode(blob)
                .map_err(|_| Erro::Fonte("sessão ilegível vinda da ponte".into()))?;
            sessao.salvar(&bruto)
        }
        Evento::Desconectado { .. } => Err(Erro::NaoPronta),
        Evento::Qr { .. } | Evento::Pareado => Ok(()),
    }
}

impl<C: Canal> PonteDoWhatsapp<C> {
    /// A ponte sobre `canal`, com a sessão em `sessao`.
    #[must_use]
    pub fn nova(canal: C, sessao: SessaoCifrada) -> Self {
        Self {
            canal,
            sessao,
            proximo_id: 1,
            iniciada: false,
        }
    }

    fn id(&mut self) -> u64 {
        let i = self.proximo_id;
        self.proximo_id += 1;
        i
    }

    fn pedir(&mut self, id: u64, linha: &str, prazo: Duration) -> Resultado<Resposta> {
        self.canal.enviar(linha)?;
        loop {
            let Some(l) = self.canal.receber(prazo)? else {
                return Err(Erro::Fonte("a ponte não respondeu a tempo".into()));
            };
            match ler_linha(&l) {
                Some(Linha::Resposta(r)) if r.id == id => {
                    return if r.ok {
                        Ok(r)
                    } else {
                        Err(Erro::Fonte(
                            r.erro.unwrap_or_else(|| "falha da ponte".into()),
                        ))
                    };
                }
                Some(Linha::Evento(e)) => tratar_evento(e, &self.sessao)?,
                _ => {} // resposta velha ou lixo: ignora
            }
        }
    }

    fn iniciar(&mut self) -> Resultado<()> {
        if self.iniciada {
            return Ok(());
        }
        // Sem sessão guardada não há o que ler: não é para abrir QR no worker.
        let Some(claro) = self.sessao.abrir()? else {
            return Err(Erro::NaoPronta);
        };
        let b64 = zeroize::Zeroizing::new(STANDARD.encode(claro.as_slice()));
        let id = self.id();
        let linha = serde_json::to_string(&Comando::Iniciar {
            id,
            sessao: Some(b64.as_str()),
        })
        .map_err(|e| Erro::Fonte(e.to_string()))?;
        self.pedir(id, &linha, PRAZO_DO_COMANDO)?;
        self.iniciada = true;
        Ok(())
    }

    fn comando<'a>(
        &mut self,
        montar: impl FnOnce(u64) -> Comando<'a>,
        prazo: Duration,
    ) -> Resultado<Resposta> {
        self.iniciar()?;
        let id = self.id();
        let linha = serde_json::to_string(&montar(id)).map_err(|e| Erro::Fonte(e.to_string()))?;
        self.pedir(id, &linha, prazo)
    }

    /// Desvincula o aparelho da conta e apaga a sessão e a chave locais.
    ///
    /// # Errors
    /// Falha ao apagar a sessão local. A falha da ponte em desvincular não impede o apagar
    /// local (o usuário também pode remover o aparelho no celular).
    pub fn desparear(&mut self) -> Resultado<()> {
        if self.sessao.existe() {
            let _ = self.comando(|id| Comando::Desparear { id }, PRAZO_DO_COMANDO);
        }
        self.encerrar();
        self.sessao.apagar()
    }

    /// Encerra a ponte.
    pub fn encerrar(&mut self) {
        if self.iniciada {
            if let Ok(l) = serde_json::to_string(&Comando::Encerrar {
                id: self.proximo_id,
            }) {
                let _ = self.canal.enviar(&l);
            }
            self.iniciada = false;
        }
        self.canal.encerrar();
    }
}

impl<C: Canal> FonteDeMensagens for PonteDoWhatsapp<C> {
    fn plataforma(&self) -> Plataforma {
        Plataforma::Whatsapp
    }

    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>> {
        let r = self.comando(|id| Comando::Conversas { id }, PRAZO_DO_COMANDO)?;
        serde_json::from_value(r.dados)
            .map_err(|_| Erro::Fonte("lista de conversas ilegível".into()))
    }

    fn mensagens_novas(
        &mut self,
        conversa: &str,
        desde: &Checkpoint,
        limite: usize,
    ) -> Resultado<Vec<Mensagem>> {
        let r = self.comando(
            |id| Comando::Mensagens {
                id,
                conversa,
                desde_quando: desde.quando,
                desde_id: desde.id.as_deref(),
                limite,
            },
            PRAZO_DO_COMANDO,
        )?;
        let mut m: Vec<Mensagem> = serde_json::from_value(r.dados)
            .map_err(|_| Erro::Fonte("lista de mensagens ilegível".into()))?;
        // Não confia na ponte: ordena e descarta o que não é posterior ao checkpoint.
        m.retain(|x| x.conversa == conversa && desde.antes_de(x));
        m.sort_by(|a, b| (a.quando, &a.id).cmp(&(b.quando, &b.id)));
        m.truncate(limite);
        Ok(m)
    }

    fn baixar_anexo(&mut self, mensagem: &Mensagem, destino: &Path) -> Resultado<()> {
        let alvo = destino.to_string_lossy().into_owned();
        self.comando(
            |id| Comando::Baixar {
                id,
                conversa: &mensagem.conversa,
                mensagem: &mensagem.id,
                destino: &alvo,
            },
            PRAZO_DO_DOWNLOAD,
        )?;
        if destino.is_file() {
            Ok(())
        } else {
            Err(Erro::Fonte("a ponte não gravou a mídia".into()))
        }
    }
}

impl<C: Canal> Drop for PonteDoWhatsapp<C> {
    fn drop(&mut self) {
        self.encerrar();
    }
}

/// O que o pareamento mostra à janela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passo {
    /// Desenhe este QR (renova a cada ~20 s) e peça ao usuário para escanear em WhatsApp ›
    /// Aparelhos conectados.
    Qr(String),
    /// Pareado: a sessão já está guardada cifrada.
    Pareado,
    /// Não deu: motivo curto, sem dado de conversa.
    Falhou(String),
}

/// O pareamento por QR, conduzido pela janela (`sondar` a cada quadro).
pub struct Pareamento<C: Canal> {
    canal: C,
    sessao: SessaoCifrada,
    pronto: bool,
}

impl<C: Canal> std::fmt::Debug for Pareamento<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pareamento")
            .field("pronto", &self.pronto)
            .finish_non_exhaustive()
    }
}

impl<C: Canal> Pareamento<C> {
    /// Começa: manda `iniciar` **sem sessão**, o que põe a ponte em modo de QR.
    ///
    /// # Errors
    /// A ponte morreu.
    pub fn iniciar(mut canal: C, sessao: SessaoCifrada) -> Resultado<Self> {
        let l = serde_json::to_string(&Comando::Iniciar {
            id: 1,
            sessao: None,
        })
        .map_err(|e| Erro::Fonte(e.to_string()))?;
        canal.enviar(&l)?;
        Ok(Self {
            canal,
            sessao,
            pronto: false,
        })
    }

    /// Lê o que a ponte tem, sem esperar. Devolve os passos novos, em ordem.
    pub fn sondar(&mut self) -> Vec<Passo> {
        let mut passos = Vec::new();
        while !self.pronto {
            match self.canal.receber(Duration::ZERO) {
                Ok(Some(l)) => match ler_linha(&l) {
                    Some(Linha::Evento(Evento::Qr { codigo })) => passos.push(Passo::Qr(codigo)),
                    Some(Linha::Evento(Evento::Sessao { blob })) => {
                        let r = STANDARD
                            .decode(blob)
                            .map_err(|_| Erro::SessaoInvalida)
                            .and_then(|b| self.sessao.salvar(&b));
                        if let Err(e) = r {
                            self.pronto = true;
                            passos.push(Passo::Falhou(e.to_string()));
                        }
                    }
                    Some(Linha::Evento(Evento::Pareado)) => {
                        self.pronto = true;
                        passos.push(if self.sessao.existe() {
                            Passo::Pareado
                        } else {
                            Passo::Falhou("a ponte não entregou a sessão".into())
                        });
                    }
                    Some(Linha::Evento(Evento::Desconectado { .. })) => {
                        self.pronto = true;
                        passos.push(Passo::Falhou("a conexão caiu".into()));
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(e) => {
                    self.pronto = true;
                    passos.push(Passo::Falhou(e.to_string()));
                }
            }
        }
        passos
    }

    /// Terminou (pareou ou falhou)?
    #[must_use]
    pub fn terminou(&self) -> bool {
        self.pronto
    }

    /// Cancela e encerra a ponte (sem apagar uma sessão já pareada).
    pub fn cancelar(&mut self) {
        self.pronto = true;
        self.canal.encerrar();
    }
}

impl<C: Canal> Drop for Pareamento<C> {
    fn drop(&mut self) {
        self.canal.encerrar();
    }
}

/// Onde o usuário aponta a ponte, se aponta (`ponte_do_whatsapp` em `mensageiros.json`).
#[must_use]
pub fn programa_da_ponte(configurado: Option<&PathBuf>) -> Option<PathBuf> {
    configurado.filter(|p| p.is_file()).cloned()
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod testes {
    use super::*;
    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;
    use sbf_chaveiro::Chaveiro;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// Canal falso: responde de um roteiro e guarda o que recebeu.
    #[derive(Clone, Default)]
    struct Falso {
        enviados: Arc<Mutex<Vec<String>>>,
        a_entregar: Arc<Mutex<VecDeque<String>>>,
        /// Para cada `cmd`, a linha de resposta (com `%ID%`).
        respostas: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl Canal for Falso {
        fn enviar(&mut self, linha: &str) -> Resultado<()> {
            self.enviados.lock().unwrap().push(linha.to_owned());
            let v: serde_json::Value = serde_json::from_str(linha).unwrap();
            let cmd = v["cmd"].as_str().unwrap().to_owned();
            let id = v["id"].as_u64().unwrap();
            for (c, r) in self.respostas.lock().unwrap().iter() {
                if *c == cmd {
                    self.a_entregar
                        .lock()
                        .unwrap()
                        .push_back(r.replace("%ID%", &id.to_string()));
                }
            }
            Ok(())
        }
        fn receber(&mut self, _p: Duration) -> Resultado<Option<String>> {
            Ok(self.a_entregar.lock().unwrap().pop_front())
        }
        fn encerrar(&mut self) {}
    }

    fn sessao(pasta: &Path) -> SessaoCifrada {
        let c = Chaveiro::abrir_com_chaveiro(
            &pasta.join("chaveiro"),
            Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap();
        SessaoCifrada::nova(pasta, c)
    }

    #[test]
    fn sem_sessao_a_ponte_nao_sobe_nem_abre_qr() {
        let d = tempfile::tempdir().unwrap();
        let falso = Falso::default();
        let mut p = PonteDoWhatsapp::nova(falso.clone(), sessao(d.path()));
        assert!(matches!(p.listar_conversas(), Err(Erro::NaoPronta)));
        assert!(falso.enviados.lock().unwrap().is_empty());
    }

    #[test]
    fn lista_conversas_e_mensagens_com_sessao_decifrada_so_pelo_canal() {
        let d = tempfile::tempdir().unwrap();
        let s = sessao(d.path());
        s.salvar(b"sessao-ficticia").unwrap();
        let falso = Falso::default();
        {
            let mut r = falso.respostas.lock().unwrap();
            r.push(("iniciar".into(), r#"{"id":%ID%,"ok":true}"#.into()));
            r.push((
                "conversas".into(),
                r#"{"id":%ID%,"ok":true,"dados":[{"id":"5511@s.whatsapp.net","nome":"Mãe","grupo":false}]}"#.into(),
            ));
            r.push((
                "mensagens".into(),
                // fora de ordem, uma antiga e uma de outra conversa: a fonte não confia
                r#"{"id":%ID%,"ok":true,"dados":[
                  {"id":"b","conversa":"c1","quando":20,"texto":"boleto"},
                  {"id":"a","conversa":"c1","quando":10,"texto":"velha"},
                  {"id":"z","conversa":"outra","quando":30,"texto":"x"},
                  {"id":"c","conversa":"c1","quando":15,"anexo":{"nome":"b.pdf"}}]}"#
                    .replace('\n', " "),
            ));
        }
        let mut p = PonteDoWhatsapp::nova(falso.clone(), s);
        let conv = p.listar_conversas().unwrap();
        assert_eq!(conv[0].nome, "Mãe");
        let m = p
            .mensagens_novas(
                "c1",
                &Checkpoint {
                    quando: 10,
                    id: Some("a".into()),
                },
                10,
            )
            .unwrap();
        assert_eq!(
            m.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["c", "b"]
        );
        let enviados = falso.enviados.lock().unwrap();
        // a sessão em base64 foi só no `iniciar`, e uma vez
        assert_eq!(
            enviados.iter().filter(|l| l.contains("\"sessao\"")).count(),
            1
        );
        assert!(enviados[0].contains(&STANDARD.encode(b"sessao-ficticia")));
    }

    #[test]
    fn evento_de_sessao_nova_e_guardado_cifrado_e_queda_vira_erro() {
        let d = tempfile::tempdir().unwrap();
        let s = sessao(d.path());
        s.salvar(b"antiga").unwrap();
        let falso = Falso::default();
        {
            let mut r = falso.respostas.lock().unwrap();
            r.push(("iniciar".into(), r#"{"id":%ID%,"ok":true}"#.into()));
            r.push((
                "conversas".into(),
                format!(
                    r#"{{"evento":"sessao","blob":"{}"}}"#,
                    STANDARD.encode(b"nova")
                ),
            ));
            r.push((
                "conversas".into(),
                r#"{"id":%ID%,"ok":true,"dados":[]}"#.into(),
            ));
        }
        let mut p = PonteDoWhatsapp::nova(falso.clone(), s.clone());
        p.listar_conversas().unwrap();
        assert_eq!(s.abrir().unwrap().unwrap().as_slice(), b"nova");

        falso.respostas.lock().unwrap().clear();
        falso.respostas.lock().unwrap().push((
            "conversas".into(),
            r#"{"evento":"desconectado","motivo":"vinculo-desfeito"}"#.into(),
        ));
        assert!(matches!(p.listar_conversas(), Err(Erro::NaoPronta)));
    }

    #[test]
    fn falha_da_ponte_vira_erro_de_fonte_sem_conteudo() {
        let d = tempfile::tempdir().unwrap();
        let s = sessao(d.path());
        s.salvar(b"x").unwrap();
        let falso = Falso::default();
        {
            let mut r = falso.respostas.lock().unwrap();
            r.push(("iniciar".into(), r#"{"id":%ID%,"ok":true}"#.into()));
            r.push((
                "baixar".into(),
                r#"{"id":%ID%,"ok":false,"erro":"midia-expirada"}"#.into(),
            ));
        }
        let mut p = PonteDoWhatsapp::nova(falso, s);
        let m = Mensagem {
            id: "1".into(),
            conversa: "c".into(),
            quando: 1,
            texto: None,
            anexo: None,
        };
        let e = p.baixar_anexo(&m, &d.path().join("x")).unwrap_err();
        assert!(matches!(e, Erro::Fonte(t) if t == "midia-expirada"));
    }

    #[test]
    fn pareamento_mostra_qr_guarda_a_sessao_e_termina() {
        let d = tempfile::tempdir().unwrap();
        let s = sessao(d.path());
        let falso = Falso::default();
        for l in [
            r#"{"evento":"qr","codigo":"2@AAA"}"#.to_owned(),
            r#"{"evento":"qr","codigo":"2@BBB"}"#.to_owned(),
            format!(
                r#"{{"evento":"sessao","blob":"{}"}}"#,
                STANDARD.encode(b"s1")
            ),
            r#"{"evento":"pareado"}"#.to_owned(),
        ] {
            falso.a_entregar.lock().unwrap().push_back(l);
        }
        let mut par = Pareamento::iniciar(falso.clone(), s.clone()).unwrap();
        // o `iniciar` do pareamento não leva sessão
        assert!(!falso.enviados.lock().unwrap()[0].contains("sessao"));
        let passos = par.sondar();
        assert_eq!(
            passos,
            [
                Passo::Qr("2@AAA".into()),
                Passo::Qr("2@BBB".into()),
                Passo::Pareado
            ]
        );
        assert!(par.terminou());
        assert_eq!(s.abrir().unwrap().unwrap().as_slice(), b"s1");
    }

    #[test]
    fn pareamento_sem_sessao_entregue_falha() {
        let d = tempfile::tempdir().unwrap();
        let falso = Falso::default();
        falso
            .a_entregar
            .lock()
            .unwrap()
            .push_back(r#"{"evento":"pareado"}"#.into());
        let mut par = Pareamento::iniciar(falso, sessao(d.path())).unwrap();
        assert!(matches!(par.sondar()[0], Passo::Falhou(_)));
    }

    #[test]
    fn desparear_apaga_sessao_e_chave_mesmo_se_a_ponte_falhar() {
        let d = tempfile::tempdir().unwrap();
        let s = sessao(d.path());
        s.salvar(b"x").unwrap();
        let falso = Falso::default(); // não responde nada: a ponte "falha"
        let mut p = PonteDoWhatsapp::nova(falso, s.clone());
        p.desparear().unwrap();
        assert!(!s.existe());
    }

    #[cfg(unix)]
    #[test]
    fn processo_externo_de_verdade_responde_linha_a_linha() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = tempfile::tempdir().unwrap();
        let script = d.path().join("ponte-falsa.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nwhile read l; do\n id=$(printf '%s' \"$l\" | sed 's/.*\"id\":\\([0-9]*\\).*/\\1/')\n printf '{\"id\":%s,\"ok\":true,\"dados\":[]}\\n' \"$id\"\ndone\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let s = sessao(d.path());
        s.salvar(b"x").unwrap();
        let mut p = PonteDoWhatsapp::nova(ProcessoExterno::subir(&script).unwrap(), s);
        assert!(p.listar_conversas().unwrap().is_empty());
        p.encerrar();
    }
}
