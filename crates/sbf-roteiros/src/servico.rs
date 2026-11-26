//! O serviço dos roteiros: junta o banco, o Chrome, o chaveiro, o inventário e a IA. É o que o
//! `sbf worker` (roteiros agendados) e a janela (Executar, Gravar novo) chamam.
//!
//! Onde ficam as coisas (tudo na pasta de dados do app, **fora do cofre**, exceto o que
//! é baixado, que vai direto para o cofre como achado):
//! - `roteiros.sqlite`: roteiros, versões, execuções e eventos;
//! - `roteiros/perfil`: o perfil dedicado do Chrome (cookies do site ficam aí);
//! - `roteiros/execucoes/<n>`: a pasta onde os downloads da execução `n` caem antes de irem ao
//!   cofre.

use crate::banco::{BancoDeRoteiros, EstadoDaExecucao, EstadoEscrito, VersaoEscrita};
use crate::chrome::{Chrome, OpcoesDoChrome};
use crate::codigo_email::{CaixaDeEmail, Relogio, RelogioReal};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::escrito::{Entradas, OpcoesEscrito, PausaDoEscrito, SbfVerbos, fim_do_roteiro};
use crate::executor::{Contexto, Fim, FonteDeSenhas, Opcoes, Pausa, PausaNoBanco, Relatorio};
use crate::gravacao::{Aprovador, FimDaGravacao, Guia, executar_com_conserto, gravar_guiado};
use crate::inventario::NoInventario;
use crate::manifesto::Mfa;
use crate::navegador::Navegador;
use bancada_navegador::luau::{self, Limites};
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
    for aviso in chrome.avisos() {
        let _ = banco.evento(execucao, None, "aviso", aviso);
    }
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

/// O que uma execução de roteiro escrito deixou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioEscrito {
    /// Como terminou.
    pub fim: crate::executor::Fim,
    /// Arquivos baixados e quantos eram novos no inventário.
    pub baixados: u32,
    pub novos: u32,
    /// O que o roteiro registrou (já limpo de segredo).
    pub registros: Vec<String>,
}

/// O que o roteiro escrito usa além do navegador: senhas, e-mail, relógio e o modo teste.
pub struct Servicos<'a> {
    pub senhas: &'a dyn FonteDeSenhas,
    /// De onde vem o código do MFA por e-mail (`None` onde não há e-mail).
    pub caixa: Option<&'a dyn CaixaDeEmail>,
    pub relogio: &'a dyn Relogio,
    /// Como se espera o usuário numa pausa. `None`: o de verdade, que olha o `roteiros.sqlite`
    /// (a janela grava Continuar ou Cancelar); os testes passam um falso.
    pub pausa: Option<&'a dyn Pausa>,
    /// O modo teste (só o `testar` liga; produção usa [`OpcoesEscrito::default`]).
    pub opcoes: &'a OpcoesEscrito,
}

impl std::fmt::Debug for Servicos<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Servicos").finish_non_exhaustive()
    }
}

fn estado_do_fim(fim: &Fim) -> (EstadoDaExecucao, Option<String>) {
    match fim {
        Fim::Concluida => (EstadoDaExecucao::Concluida, None),
        Fim::Falhou { motivo, .. } => (EstadoDaExecucao::Falhou, Some(motivo.clone())),
        Fim::Bloqueada { bloqueio, .. } => {
            (EstadoDaExecucao::Bloqueada, Some(bloqueio.to_string()))
        }
        Fim::Cancelada => (EstadoDaExecucao::Cancelada, None),
        Fim::PausaExpirou { .. } => (
            EstadoDaExecucao::Falhou,
            Some("o usuário não voltou a tempo: execução não concluída".to_owned()),
        ),
    }
}

/// Executa a `versao` do roteiro escrito num navegador já aberto (o miolo; o Chrome vem de
/// fora). A execução `execucao` já existe no banco (`iniciar_execucao_escrita`); aqui ela é
/// finalizada com o estado em que o roteiro terminou.
///
/// # Errors
/// Falha de infraestrutura (banco, inventário, disco) ou manifesto/opções inválidos. Falha do
/// roteiro, bloqueio e pausa não resolvida vão no [`RelatorioEscrito`] e no histórico.
pub fn executar_escrito_em(
    amb: &Ambiente,
    navegador: &mut dyn Navegador,
    versao: &VersaoEscrita,
    execucao: i64,
    servicos: &Servicos<'_>,
    cancelar: &AtomicBool,
) -> Resultado<RelatorioEscrito> {
    // Revalida em tempo de execução (regra dura: MFA por e-mail só sem movimentar dinheiro).
    versao.manifesto.validar()?;
    servicos.opcoes.validar(&versao.manifesto)?;
    let banco = amb.banco()?;
    // Guarda de rascunho: com as opções de produção só roda versão ATIVA (o estado vem do
    // banco, não da struct que o chamador montou). Rascunho e testado só rodam no modo teste,
    // que é o caminho do `testar` (portal falso local).
    let modo_teste =
        !servicos.opcoes.mapa_de_hosts.is_empty() && servicos.opcoes.aceitar_http_loopback;
    if !modo_teste {
        let estado = banco.versao_escrita(&versao.roteiro, versao.numero)?.estado;
        if estado != EstadoEscrito::Ativo {
            return Err(ErroDeRoteiro::Invalido(format!(
                "a versão {} do roteiro escrito `{}` não está ativa ({}): só se executa versão ativa",
                versao.numero,
                versao.roteiro,
                estado.texto()
            )));
        }
    }
    let conn = amb.conexao_do_inventario()?;
    let mut destino = NoInventario::novo(&conn, &amb.cofre, &versao.roteiro, execucao);
    let pausa = PausaDoEscrito {
        banco: &banco,
        prazo: amb.prazo_da_pausa,
        intervalo: Duration::from_millis(500),
        cancelar,
    };
    let (fim, baixados, novos, registros) = {
        let mut verbos = SbfVerbos::novo(Entradas {
            manifesto: &versao.manifesto,
            execucao,
            navegador,
            senhas: servicos.senhas,
            caixa: servicos.caixa,
            relogio: servicos.relogio,
            pausa: servicos.pausa.unwrap_or(&pausa),
            destino: &mut destino,
            banco: &banco,
            pasta: amb.pasta_da_execucao(execucao),
            opcoes: amb.opcoes,
            teste: servicos.opcoes.clone(),
            cancelar,
        });
        let r = luau::executar(&versao.fonte, &mut verbos, &Limites::default());
        let fim = fim_do_roteiro(&r, &verbos);
        (
            fim,
            verbos.baixados(),
            verbos.novos(),
            verbos.registros().to_vec(),
        )
    };
    destino.fechar("concluida")?;
    // Sobra de download que não chegou ao inventário: não fica na pasta de dados.
    let _ = std::fs::remove_dir_all(amb.pasta_da_execucao(execucao));
    let (estado, motivo) = estado_do_fim(&fim);
    banco.finalizar_escrita(execucao, estado, motivo.as_deref(), baixados, novos)?;
    Ok(RelatorioEscrito {
        fim,
        baixados,
        novos,
        registros,
    })
}

/// Executa a versão ativa do roteiro escrito `nome`, abrindo o Chrome. Recusa se já está
/// rodando ou se não há versão ativa. O `gatilho` é "janela" (botão) ou "agenda" (worker).
///
/// # Errors
/// Sem versão ativa, Chrome que não abre, roteiro já em execução, ou falha de infraestrutura.
pub fn executar_escrito(
    amb: &Ambiente,
    nome: &str,
    gatilho: &str,
    senhas: &dyn FonteDeSenhas,
    cancelar: &AtomicBool,
) -> Resultado<RelatorioEscrito> {
    let banco = amb.banco()?;
    let Some(versao) = banco.versao_ativa_escrita(nome)? else {
        return Err(ErroDeRoteiro::Invalido(format!(
            "o roteiro escrito `{nome}` não tem versão ativa"
        )));
    };
    versao.manifesto.validar()?;
    // Atômico: recusa se outro processo (a janela, o worker) já a executa.
    let execucao = banco.iniciar_execucao_escrita(nome, versao.numero, gatilho)?;
    let falhou = |e: &ErroDeRoteiro| {
        let _ = banco.finalizar_escrita(
            execucao,
            EstadoDaExecucao::Falhou,
            Some(&e.to_string()),
            0,
            0,
        );
    };
    let caixa = if matches!(versao.manifesto.mfa, Mfa::Email { .. }) {
        senhas.caixa_de_email()
    } else {
        None
    };
    let mut chrome = match amb.abrir_chrome() {
        Ok(c) => c,
        Err(e) => {
            falhou(&e);
            return Err(e);
        }
    };
    // Avisos da abertura (por exemplo, o Chrome sem sandbox por `SBF_CHROME_SEM_SANDBOX`), uma
    // vez por execução e sem dado sensível.
    for aviso in chrome.avisos() {
        let _ = banco.evento_escrito(execucao, "aviso", aviso);
    }
    let r = executar_escrito_em(
        amb,
        &mut chrome,
        &versao,
        execucao,
        &Servicos {
            senhas,
            caixa: caixa.as_deref(),
            relogio: &RelogioReal,
            pausa: None,
            opcoes: &OpcoesEscrito::default(),
        },
        cancelar,
    );
    chrome.fechar();
    if let Err(e) = &r {
        // Falha de infraestrutura no meio: a execução não pode ficar "rodando".
        falhou(e);
    }
    r
}

/// Executa os roteiros escritos ativos cuja agenda venceu (um de cada vez).
///
/// # Errors
/// Só falha de infraestrutura do banco; a falha de um roteiro não impede os outros.
pub fn executar_escritos_devidos(
    amb: &Ambiente,
    agora_unix: i64,
    senhas: &dyn FonteDeSenhas,
    cancelar: &AtomicBool,
) -> Resultado<ResumoDeRoteiros> {
    executar_escritos_devidos_com(amb, agora_unix, cancelar, &mut |nome| {
        executar_escrito(amb, nome, "agenda", senhas, cancelar)
    })
}

/// Como [`executar_escritos_devidos`], com a execução de cada roteiro por `executar` (os
/// testes trocam o Chrome por um navegador falso).
///
/// # Errors
/// Só falha de infraestrutura do banco.
pub fn executar_escritos_devidos_com(
    amb: &Ambiente,
    agora_unix: i64,
    cancelar: &AtomicBool,
    executar: &mut dyn FnMut(&str) -> Resultado<RelatorioEscrito>,
) -> Resultado<ResumoDeRoteiros> {
    let banco = amb.banco()?;
    banco.encerrar_orfas_escritas()?;
    let mut resumo = ResumoDeRoteiros::default();
    for nome in banco.escritos_devidos(agora_unix)? {
        if cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        resumo.executados += 1;
        if let Ok(rel) = executar(&nome) {
            resumo.baixados += rel.baixados;
            resumo.novos += rel.novos;
            if rel.fim == Fim::Concluida {
                resumo.concluidos += 1;
            }
        }
    }
    Ok(resumo)
}
