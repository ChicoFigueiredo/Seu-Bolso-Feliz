//! O executor: roda os passos de um roteiro, sem IA.
//!
//! Garantias (cada uma tem teste):
//! - **RO-01**: a senha e o CPF só passam pelo chaveiro e pelo [`Navegador::preencher_segredo`].
//!   O executor nunca os põe em evento, erro, banco ou `Debug`; todo texto que grava passa por
//!   [`limpar_texto`], que troca por `***` qualquer segredo usado na execução.
//! - **RO-02**: todo passo de clicar ou baixar é conferido na lista de bloqueio **antes** de
//!   agir, e o bloqueio que o script do navegador barrou durante a ação aborta o roteiro. O
//!   passo de navegar também confere a URL.
//! - **RO-03**: MFA e CAPTCHA (detectados na página) e o passo explícito de pausa param o
//!   roteiro e esperam o usuário. Continuar retoma do passo certo.
//! - **RO-05**: cada arquivo baixado vai para o [`Destino`] (o inventário).

use crate::banco::{BancoDeRoteiros, EstadoDaExecucao};
use crate::bloqueio::{self, Bloqueio};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::navegador::Navegador;
use crate::pagina::MotivoDePausa;
use crate::passo::{Passo, Roteiro};
use sbf_chaveiro::Segredo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// De onde vêm a senha do site e o CPF (o chaveiro; nos testes, um falso).
pub trait FonteDeSenhas {
    /// A senha do site de rótulo `rotulo`.
    ///
    /// # Errors
    /// Texto sem o valor.
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String>;
    /// O CPF.
    ///
    /// # Errors
    /// Texto sem o valor.
    fn cpf(&self) -> Result<Segredo, String>;
}

impl FonteDeSenhas for sbf_chaveiro::Chaveiro {
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String> {
        sbf_chaveiro::Chaveiro::senha_de_site(self, rotulo)
            .map_err(|_| format!("a senha do site \"{rotulo}\" não está no chaveiro"))
    }
    fn cpf(&self) -> Result<Segredo, String> {
        sbf_chaveiro::Chaveiro::cpf(self).map_err(|_| "o CPF não está no chaveiro".to_owned())
    }
}

/// O que o usuário decidiu na pausa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    Continuar,
    Cancelar,
    /// O usuário não voltou a tempo.
    Expirou,
}

/// Como a execução espera o usuário.
pub trait Pausa {
    /// Bloqueia até o usuário decidir. A execução já está marcada como aguardando.
    fn aguardar(&self, execucao: i64, motivo: &str) -> Decisao;
}

/// A pausa de verdade: olha o campo `continuar` do `roteiros.sqlite`, que a janela grava.
#[derive(Debug)]
pub struct PausaNoBanco<'a> {
    pub banco: &'a BancoDeRoteiros,
    pub prazo: Duration,
    pub intervalo: Duration,
    pub cancelar: &'a AtomicBool,
}

impl Pausa for PausaNoBanco<'_> {
    fn aguardar(&self, execucao: i64, _motivo: &str) -> Decisao {
        let inicio = Instant::now();
        loop {
            if self.cancelar.load(Ordering::SeqCst) {
                return Decisao::Cancelar;
            }
            match self.banco.pedido_do_usuario(execucao) {
                Ok(1) => return Decisao::Continuar,
                Ok(0) | Err(_) => {}
                Ok(_) => return Decisao::Cancelar,
            }
            if inicio.elapsed() >= self.prazo {
                return Decisao::Expirou;
            }
            std::thread::sleep(self.intervalo);
        }
    }
}

/// O que aconteceu com um arquivo entregue ao destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recebido {
    /// Achado novo no inventário.
    Novo,
    /// O mesmo conteúdo (sha256) já estava lá.
    Repetido,
}

/// Para onde vai o que foi baixado (o inventário, ADR-014).
pub trait Destino {
    /// Registra o arquivo como achado de origem `roteiro:<nome>/<execução>/<arquivo>`.
    ///
    /// # Errors
    /// Falha do banco ou do disco.
    fn receber(&mut self, arquivo: &Path, nome: &str) -> Resultado<Recebido>;
}

/// Como a execução terminou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fim {
    Concluida,
    /// Um passo falhou (o site mudou, prazo, seletor).
    Falhou { passo: usize, motivo: String },
    /// A lista de bloqueio barrou uma ação.
    Bloqueada { passo: usize, bloqueio: Bloqueio },
    Cancelada,
    /// O usuário não voltou da pausa a tempo.
    PausaExpirou { passo: usize },
}

/// O relatório de uma execução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relatorio {
    pub fim: Fim,
    pub baixados: u32,
    pub novos: u32,
    /// Quantas pausas houve.
    pub pausas: u32,
}

/// Prazos.
#[derive(Debug, Clone, Copy)]
pub struct Opcoes {
    /// Quanto esperar um seletor aparecer.
    pub espera: Duration,
    /// Quanto esperar um download.
    pub download: Duration,
    /// Intervalo entre conferências de seletor.
    pub intervalo: Duration,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            espera: Duration::from_secs(20),
            download: Duration::from_secs(60),
            intervalo: Duration::from_millis(150),
        }
    }
}

/// Tudo de que a execução precisa.
pub struct Contexto<'a> {
    pub navegador: &'a mut dyn Navegador,
    pub senhas: &'a dyn FonteDeSenhas,
    pub pausa: &'a dyn Pausa,
    pub destino: &'a mut dyn Destino,
    pub banco: &'a BancoDeRoteiros,
    /// A pasta onde os downloads desta execução caem.
    pub pasta: PathBuf,
    pub opcoes: Opcoes,
    pub cancelar: &'a AtomicBool,
}

impl std::fmt::Debug for Contexto<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Contexto")
            .field("pasta", &self.pasta)
            .finish_non_exhaustive()
    }
}

/// Troca por `***` todo segredo (e a forma percent-encoded) que apareça em `texto`.
#[must_use]
pub fn limpar_texto(texto: &str, segredos: &[Segredo]) -> String {
    let mut saida = texto.to_owned();
    for s in segredos {
        let v = s.expor();
        if v.chars().count() >= 4 && saida.contains(v) {
            saida = saida.replace(v, "***");
        }
    }
    saida
}

/// Interage com a página (vale a detecção de MFA e CAPTCHA antes do passo).
fn interage(p: &Passo) -> bool {
    !matches!(p, Passo::Navegar { .. } | Passo::PausaParaUsuario { .. })
}

/// Executa o roteiro desde o passo 0.
///
/// # Errors
/// Só falha de infraestrutura (banco, disco). Falha de passo e bloqueio vêm em [`Fim`].
pub fn executar(contexto: &mut Contexto<'_>, execucao: i64, roteiro: &Roteiro) -> Resultado<Relatorio> {
    let mut usados: Vec<Segredo> = Vec::new();
    let r = rodar(contexto, execucao, roteiro, &mut usados);
    // Segredo nenhum fica na memória além da execução (Segredo zera ao cair).
    drop(usados);
    r
}

fn rodar(
    c: &mut Contexto<'_>,
    execucao: i64,
    roteiro: &Roteiro,
    usados: &mut Vec<Segredo>,
) -> Resultado<Relatorio> {
    let mut rel = Relatorio { fim: Fim::Concluida, baixados: 0, novos: 0, pausas: 0 };
    let mut i = 0_usize;
    // O passo em que o usuário acabou de resolver a pausa: não detecta de novo antes dele.
    let mut liberado: Option<usize> = None;
    while i < roteiro.passos.len() {
        if c.cancelar.load(Ordering::SeqCst) {
            rel.fim = Fim::Cancelada;
            break;
        }
        let passo = &roteiro.passos[i];
        c.banco.atualizar_passo(execucao, i)?;

        if let Passo::PausaParaUsuario { motivo } = passo {
            match pausar(c, execucao, i, motivo, &mut rel)? {
                Decisao::Continuar => {
                    i += 1;
                    continue;
                }
                outra => {
                    rel.fim = fim_da_pausa(outra, i);
                    break;
                }
            }
        }

        if interage(passo) && liberado != Some(i) {
            let motivo = c.navegador.pagina().ok().and_then(|p| p.pausa_necessaria());
            if let Some(m) = motivo {
                match pausar(c, execucao, i, m.frase(), &mut rel)? {
                    Decisao::Continuar => {
                        // Volta ao MESMO passo: o que veio antes (o login) não se repete.
                        liberado = Some(i);
                        continue;
                    }
                    outra => {
                        rel.fim = fim_da_pausa(outra, i);
                        break;
                    }
                }
            }
        }

        c.banco.evento(execucao, Some(i), "passo", &limpar_texto(&passo.descricao(), usados))?;
        match executar_passo(c, execucao, i, passo, usados, &mut rel) {
            Ok(()) => i += 1,
            Err(ErroDeRoteiro::Bloqueado(b)) => {
                c.banco.evento(execucao, Some(i), "bloqueio", &b.to_string())?;
                rel.fim = Fim::Bloqueada { passo: i, bloqueio: b };
                break;
            }
            Err(ErroDeRoteiro::Cancelado) => {
                rel.fim = Fim::Cancelada;
                break;
            }
            Err(ErroDeRoteiro::Banco(e)) => return Err(ErroDeRoteiro::Banco(e)),
            Err(e) => {
                let motivo = limpar_texto(&e.to_string(), usados);
                c.banco.evento(execucao, Some(i), "falha", &motivo)?;
                rel.fim = Fim::Falhou { passo: i, motivo };
                break;
            }
        }
    }
    let (estado, motivo) = match &rel.fim {
        Fim::Concluida => (EstadoDaExecucao::Concluida, None),
        Fim::Falhou { passo, motivo } => {
            (EstadoDaExecucao::Falhou, Some(format!("passo {}: {motivo}", passo + 1)))
        }
        Fim::Bloqueada { passo, bloqueio } => {
            (EstadoDaExecucao::Bloqueada, Some(format!("passo {}: {bloqueio}", passo + 1)))
        }
        Fim::Cancelada => (EstadoDaExecucao::Cancelada, None),
        Fim::PausaExpirou { passo } => (
            EstadoDaExecucao::Falhou,
            Some(format!("passo {}: o usuário não voltou a tempo", passo + 1)),
        ),
    };
    c.banco
        .finalizar(execucao, estado, motivo.as_deref(), rel.baixados, rel.novos)?;
    Ok(rel)
}

fn fim_da_pausa(d: Decisao, passo: usize) -> Fim {
    match d {
        Decisao::Expirou => Fim::PausaExpirou { passo },
        _ => Fim::Cancelada,
    }
}

fn pausar(
    c: &mut Contexto<'_>,
    execucao: i64,
    passo: usize,
    motivo: &str,
    rel: &mut Relatorio,
) -> Resultado<Decisao> {
    rel.pausas += 1;
    c.banco.marcar_aguardando(execucao, motivo)?;
    c.banco.evento(execucao, Some(passo), "pausa", motivo)?;
    let d = c.pausa.aguardar(execucao, motivo);
    if d == Decisao::Continuar {
        c.banco.marcar_rodando(execucao)?;
        c.banco.evento(execucao, Some(passo), "retomada", "o usuário continuou")?;
    }
    Ok(d)
}

fn falha(passo: usize, motivo: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::PassoFalhou { passo, motivo: motivo.into() }
}

/// Espera o seletor ficar visível.
fn esperar_visivel(c: &mut Contexto<'_>, passo: usize, seletor: &str, prazo: Duration) -> Resultado<()> {
    let inicio = Instant::now();
    loop {
        if c.navegador.visivel(seletor)? {
            return Ok(());
        }
        if c.cancelar.load(Ordering::SeqCst) {
            return Err(ErroDeRoteiro::Cancelado);
        }
        if inicio.elapsed() >= prazo {
            return Err(falha(passo, format!("o elemento \"{seletor}\" não apareceu")));
        }
        std::thread::sleep(c.opcoes.intervalo);
    }
}

/// Confere o elemento na lista de bloqueio antes de agir.
fn conferir_elemento(c: &mut Contexto<'_>, passo: usize, seletor: &str) -> Resultado<crate::bloqueio::Candidato> {
    let Some(cand) = c.navegador.descrever(seletor)? else {
        return Err(falha(passo, format!("o elemento \"{seletor}\" não existe")));
    };
    if let Some(b) = bloqueio::avaliar(&cand) {
        return Err(ErroDeRoteiro::Bloqueado(b));
    }
    Ok(cand)
}

/// Depois de agir: o script do navegador barrou algo?
fn conferir_barrado(c: &mut Contexto<'_>) -> Resultado<()> {
    if let Some(b) = c.navegador.bloqueios()?.into_iter().next() {
        return Err(ErroDeRoteiro::Bloqueado(b));
    }
    Ok(())
}

fn nome_de_arquivo_seguro(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let limpo = limpo.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_owned();
    if limpo.is_empty() { "arquivo".to_owned() } else { limpo.chars().take(120).collect() }
}

fn csv(linhas: &[Vec<String>]) -> String {
    let mut s = String::new();
    for l in linhas {
        let campos: Vec<String> = l
            .iter()
            .map(|c| {
                if c.contains([',', '"', '\n', '\r', ';']) {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            })
            .collect();
        s.push_str(&campos.join(","));
        s.push('\n');
    }
    s
}

fn executar_passo(
    c: &mut Contexto<'_>,
    _execucao: i64,
    i: usize,
    passo: &Passo,
    usados: &mut Vec<Segredo>,
    rel: &mut Relatorio,
) -> Resultado<()> {
    match passo {
        Passo::Navegar { url } => {
            if let Some(b) = bloqueio::avaliar_url(url) {
                return Err(ErroDeRoteiro::Bloqueado(b));
            }
            c.navegador.navegar(url)?;
            conferir_barrado(c)
        }
        Passo::EsperarSeletor { seletor, segundos } => {
            let prazo = segundos.map_or(c.opcoes.espera, Duration::from_secs);
            esperar_visivel(c, i, seletor, prazo)
        }
        Passo::Clicar { seletor } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            conferir_elemento(c, i, seletor)?;
            c.navegador.clicar(seletor)?;
            conferir_barrado(c)
        }
        Passo::Preencher { seletor, texto } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            let cand = conferir_elemento(c, i, seletor)?;
            if cand.eh_senha {
                return Err(falha(i, "campo de senha só se preenche com a senha do chaveiro"));
            }
            c.navegador.preencher(seletor, texto)
        }
        Passo::PreencherSenha { seletor, rotulo } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            let segredo = c.senhas.senha_de_site(rotulo).map_err(ErroDeRoteiro::Chaveiro)?;
            c.navegador.preencher_segredo(seletor, &segredo)?;
            usados.push(segredo);
            Ok(())
        }
        Passo::PreencherCpf { seletor } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            let segredo = c.senhas.cpf().map_err(ErroDeRoteiro::Chaveiro)?;
            c.navegador.preencher_segredo(seletor, &segredo)?;
            usados.push(segredo);
            Ok(())
        }
        Passo::Baixar { seletor, nome } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            conferir_elemento(c, i, seletor)?;
            std::fs::create_dir_all(&c.pasta)?;
            let caminho = c.navegador.clicar_e_baixar(seletor, &c.pasta, c.opcoes.download)?;
            conferir_barrado(c)?;
            let nome_final = match nome {
                Some(n) => {
                    let ext = caminho.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let base = nome_de_arquivo_seguro(n);
                    if ext.is_empty() || base.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase())) {
                        base
                    } else {
                        format!("{base}.{ext}")
                    }
                }
                None => nome_de_arquivo_seguro(
                    caminho.file_name().and_then(|n| n.to_str()).unwrap_or("arquivo"),
                ),
            };
            entregar(c, rel, &caminho, &nome_final)
        }
        Passo::LerTabela { seletor, nome } => {
            esperar_visivel(c, i, seletor, c.opcoes.espera)?;
            let linhas = c.navegador.ler_tabela(seletor)?;
            std::fs::create_dir_all(&c.pasta)?;
            let nome_final = format!("{}.csv", nome_de_arquivo_seguro(nome));
            let caminho = c.pasta.join(&nome_final);
            std::fs::write(&caminho, csv(&linhas))?;
            entregar(c, rel, &caminho, &nome_final)
        }
        Passo::PausaParaUsuario { .. } => Ok(()),
    }
}

fn entregar(c: &mut Contexto<'_>, rel: &mut Relatorio, caminho: &Path, nome: &str) -> Resultado<()> {
    let r = c.destino.receber(caminho, nome)?;
    rel.baixados += 1;
    if r == Recebido::Novo {
        rel.novos += 1;
    }
    Ok(())
}

/// Um motivo de pausa vindo do passo explícito ou da detecção, para o histórico.
#[must_use]
pub fn frase_da_pausa(m: MotivoDePausa) -> &'static str {
    m.frase()
}
