//! O serviço dos roteiros: junta o banco, o Chrome, o chaveiro, o inventário e a IA. É o que o
//! `sbf worker` (roteiros agendados) e a janela (Executar, Gravar novo) chamam.
//!
//! Onde ficam as coisas (tudo na pasta de dados do app, **fora do cofre**, exceto o que
//! é baixado, que vai direto para o cofre como achado):
//! - `roteiros.sqlite`: roteiros, versões, execuções e eventos;
//! - `roteiros/perfil`: o perfil dedicado do Chrome (cookies do site ficam aí);
//! - `roteiros/execucoes/<n>`: a pasta onde os downloads da execução `n` caem antes de irem ao
//!   cofre.

use crate::banco::{BancoDeRoteiros, EstadoDaExecucao};
use crate::chrome::{Chrome, OpcoesDoChrome};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::executor::{Contexto, FonteDeSenhas, Opcoes, PausaNoBanco, Relatorio};
use crate::gravacao::{Aprovador, FimDaGravacao, Guia, executar_com_conserto, gravar_guiado};
use crate::inventario::NoInventario;
use crate::navegador::Navegador;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Onde tudo mora.
#[derive(Debug, Clone)]
pub struct Ambiente {
    /// A pasta de dados do app.
    pub pasta_dados: PathBuf,
    /// A pasta do cofre.
    pub cofre: PathBuf,
    /// O `inventario.sqlite`.
    pub inventario: PathBuf,
    /// O Chrome (senão, o que for achado).
    pub programa_do_chrome: Option<PathBuf>,
    /// Sem janela (só testes e demonstração).
    pub sem_janela: bool,
    /// Prazos de espera dos passos.
    pub opcoes: Opcoes,
    /// Quanto esperar o usuário numa pausa.
    pub prazo_da_pausa: Duration,
}

impl Ambiente {
    /// O ambiente com os prazos de uso real (a pausa espera 30 minutos).
    #[must_use]
    pub fn novo(pasta_dados: &Path, cofre: &Path) -> Self {
        Self {
            pasta_dados: pasta_dados.to_path_buf(),
            cofre: cofre.to_path_buf(),
            inventario: cofre.join("inventario.sqlite"),
            programa_do_chrome: None,
            sem_janela: false,
            opcoes: Opcoes::default(),
            prazo_da_pausa: Duration::from_secs(30 * 60),
        }
    }

    /// O `roteiros.sqlite`.
    #[must_use]
    pub fn arquivo_do_banco(&self) -> PathBuf {
        self.pasta_dados.join("roteiros.sqlite")
    }

    /// Abre o banco dos roteiros.
    ///
    /// # Errors
    /// Disco ou SQLite.
    pub fn banco(&self) -> Resultado<BancoDeRoteiros> {
        BancoDeRoteiros::abrir(&self.arquivo_do_banco())
    }

    /// O perfil dedicado do Chrome.
    #[must_use]
    pub fn perfil(&self) -> PathBuf {
        self.pasta_dados.join("roteiros").join("perfil")
    }

    /// A pasta dos downloads de uma execução.
    #[must_use]
    pub fn pasta_da_execucao(&self, execucao: i64) -> PathBuf {
        self.pasta_dados
            .join("roteiros")
            .join("execucoes")
            .join(execucao.to_string())
    }

    fn conexao_do_inventario(&self) -> Resultado<Connection> {
        let c = Connection::open(&self.inventario)?;
        c.busy_timeout(Duration::from_secs(30))?;
        c.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(c)
    }

    /// Abre o Chrome com o perfil dedicado.
    ///
    /// # Errors
    /// Chrome ausente ou que não subiu.
    pub fn abrir_chrome(&self) -> Resultado<Chrome> {
        crate::chrome::abrir(&OpcoesDoChrome {
            programa: self.programa_do_chrome.clone(),
            perfil: self.perfil(),
            sem_janela: self.sem_janela,
        })
    }
}

/// Executa o roteiro `roteiro_id` num navegador já aberto (o miolo; o Chrome vem de fora).
///
/// # Errors
/// Falha de infraestrutura. Falha de passo, bloqueio e pausa não resolvida vão no
/// [`Relatorio`] e no histórico.
pub fn executar_roteiro_em(
    amb: &Ambiente,
    navegador: &mut dyn Navegador,
    roteiro_id: i64,
    execucao: i64,
    senhas: &dyn FonteDeSenhas,
    guia: Option<&Guia<'_>>,
    cancelar: &AtomicBool,
) -> Resultado<Relatorio> {
    let banco = amb.banco()?;
    let (_, _, roteiro) = banco.versao_ativa(roteiro_id)?;
    let conn = amb.conexao_do_inventario()?;
    let mut destino = NoInventario::novo(&conn, &amb.cofre, &roteiro.nome, execucao);
    let pausa = PausaNoBanco {
        banco: &banco,
        prazo: amb.prazo_da_pausa,
        intervalo: Duration::from_millis(500),
        cancelar,
    };
    let rel = {
        let mut c = Contexto {
            navegador,
            senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: amb.pasta_da_execucao(execucao),
            opcoes: amb.opcoes,
            cancelar,
        };
        executar_com_conserto(&mut c, roteiro_id, execucao, &roteiro, guia)?
    };
    destino.fechar("concluida")?;
    // Sobra de download que não chegou ao cofre (falha no meio): não fica na pasta de dados.
    let _ = std::fs::remove_dir_all(amb.pasta_da_execucao(execucao));
    Ok(rel)
}

/// Executa o roteiro abrindo o Chrome. Recusa se ele já está rodando.
///
/// # Errors
/// Falha de infraestrutura, Chrome que não abre, ou roteiro já em execução.
pub fn executar_roteiro(
    amb: &Ambiente,
    roteiro_id: i64,
    gatilho: &str,
    senhas: &dyn FonteDeSenhas,
    guia: Option<&Guia<'_>>,
    cancelar: &AtomicBool,
) -> Resultado<Relatorio> {
    let banco = amb.banco()?;
    if banco.em_execucao(roteiro_id)? {
        return Err(ErroDeRoteiro::Invalido("o roteiro já está rodando".into()));
    }
    let (versao, _, _) = banco.versao_ativa(roteiro_id)?;
    let execucao = banco.iniciar_execucao(roteiro_id, versao, gatilho)?;
    let mut chrome = match amb.abrir_chrome() {
        Ok(c) => c,
        Err(e) => {
            banco.finalizar(
                execucao,
                EstadoDaExecucao::Falhou,
                Some(&e.to_string()),
                0,
                0,
            )?;
            return Err(e);
        }
    };
    let r = executar_roteiro_em(
        amb,
        &mut chrome,
        roteiro_id,
        execucao,
        senhas,
        guia,
        cancelar,
    );
    chrome.fechar();
    if let Err(e) = &r {
        // Falha de infraestrutura no meio: a execução não pode ficar "rodando".
        let _ = banco.finalizar(
            execucao,
            EstadoDaExecucao::Falhou,
            Some(&e.to_string()),
            0,
            0,
        );
    }
    r
}

/// O que uma passada dos roteiros agendados fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDeRoteiros {
    pub executados: usize,
    pub concluidos: usize,
    pub baixados: u32,
    pub novos: u32,
}

/// Executa os roteiros agendados que já passaram do intervalo (um de cada vez).
///
/// # Errors
/// Só falha de infraestrutura do banco; a falha de um roteiro não impede os outros.
pub fn executar_devidos(
    amb: &Ambiente,
    agora_unix: i64,
    senhas: &dyn FonteDeSenhas,
    guia: Option<&Guia<'_>>,
    cancelar: &AtomicBool,
) -> Resultado<ResumoDeRoteiros> {
    let banco = amb.banco()?;
    banco.encerrar_orfas()?;
    let mut resumo = ResumoDeRoteiros::default();
    for id in banco.devidos(agora_unix)? {
        if cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        resumo.executados += 1;
        if let Ok(rel) = executar_roteiro(amb, id, "agenda", senhas, guia, cancelar) {
            resumo.baixados += rel.baixados;
            resumo.novos += rel.novos;
            if rel.fim == crate::executor::Fim::Concluida {
                resumo.concluidos += 1;
            }
        }
    }
    Ok(resumo)
}

/// A gravação guiada de um roteiro novo: cria o rascunho, abre o Chrome e deixa a IA propor
/// passos que o `aprovador` aceita ou recusa. Pronta, a gravação vira a versão 1, ativa.
///
/// # Errors
/// Nome inválido ou repetido, Chrome que não abre, falha da IA ou de infraestrutura.
#[allow(clippy::too_many_arguments)]
pub fn gravar_novo(
    amb: &Ambiente,
    nome: &str,
    url_inicial: &str,
    rotulo_senha: Option<&str>,
    objetivo: &str,
    senhas: &dyn FonteDeSenhas,
    guia: &Guia<'_>,
    aprovador: &mut dyn Aprovador,
    cancelar: &AtomicBool,
) -> Resultado<(i64, FimDaGravacao)> {
    let banco = amb.banco()?;
    let id = banco.criar_rascunho(nome, objetivo, url_inicial, rotulo_senha)?;
    let (versao, _, _) = banco.versao_ativa(id)?;
    let execucao = banco.iniciar_execucao(id, versao, "gravacao")?;
    let feito = (|| -> Resultado<(Vec<crate::passo::Passo>, FimDaGravacao)> {
        let mut chrome = amb.abrir_chrome()?;
        let conn = amb.conexao_do_inventario()?;
        let mut destino = NoInventario::novo(&conn, &amb.cofre, nome, execucao);
        let pausa = PausaNoBanco {
            banco: &banco,
            prazo: amb.prazo_da_pausa,
            intervalo: Duration::from_millis(500),
            cancelar,
        };
        let r = {
            let mut c = Contexto {
                navegador: &mut chrome,
                senhas,
                pausa: &pausa,
                destino: &mut destino,
                banco: &banco,
                pasta: amb.pasta_da_execucao(execucao),
                opcoes: amb.opcoes,
                cancelar,
            };
            gravar_guiado(guia, &mut c, execucao, objetivo, url_inicial, aprovador, 40)
        };
        destino.fechar("concluida")?;
        chrome.fechar();
        let _ = std::fs::remove_dir_all(amb.pasta_da_execucao(execucao));
        let g = r?;
        Ok((g.passos, g.fim))
    })();
    match feito {
        Ok((passos, fim)) if passos.len() > 1 => {
            banco.concluir_gravacao(id, &passos)?;
            banco.finalizar(execucao, EstadoDaExecucao::Concluida, None, 0, 0)?;
            Ok((id, fim))
        }
        Ok((_, fim)) => {
            // Nada gravado além de abrir a página: não deixa um roteiro vazio.
            banco.apagar_roteiro(id)?;
            Ok((0, fim))
        }
        Err(e) => {
            let _ = banco.finalizar(
                execucao,
                EstadoDaExecucao::Falhou,
                Some(&e.to_string()),
                0,
                0,
            );
            banco.apagar_roteiro(id)?;
            Err(e)
        }
    }
}

/// O que uma execução de roteiro escrito deixou (a frente de execução preenche).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioEscrito {
    /// Como terminou.
    pub fim: crate::executor::Fim,
    /// Arquivos baixados e quantos eram novos no inventário.
    pub baixados: u32,
    pub novos: u32,
    /// O que o roteiro registrou (sem segredo).
    pub registros: Vec<String>,
}

/// Executa a versão ativa do roteiro escrito `nome`, abrindo o Chrome. Recusa se já está
/// rodando ou se não há versão ativa.
///
/// # Errors
/// Sem versão ativa, Chrome que não abre, ou falha de infraestrutura.
pub fn executar_escrito(
    _amb: &Ambiente,
    _nome: &str,
    _gatilho: &str,
    _senhas: &dyn FonteDeSenhas,
    _cancelar: &AtomicBool,
) -> Resultado<RelatorioEscrito> {
    Err(ErroDeRoteiro::Invalido(
        "a execução do roteiro escrito ainda não está ligada".into(),
    ))
}

/// Executa os roteiros escritos ativos cuja agenda venceu (um de cada vez).
///
/// # Errors
/// Só falha de infraestrutura do banco; a falha de um roteiro não impede os outros.
pub fn executar_escritos_devidos(
    _amb: &Ambiente,
    _agora_unix: i64,
    _senhas: &dyn FonteDeSenhas,
    _cancelar: &AtomicBool,
) -> Resultado<ResumoDeRoteiros> {
    Ok(ResumoDeRoteiros::default())
}
