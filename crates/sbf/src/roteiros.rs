//! Os roteiros de verdade no `sbf worker` (spec 10, C1): executa os agendados com o Chrome e o
//! chaveiro do usuário e, se um passo falha, pede à IA (o `claude` do usuário, via o mesmo motor
//! da triagem) um conserto que **fica como proposta** até o usuário aprovar na janela.
//!
//! Também é o CLI do **roteiro escrito** (Luau), `sbf roteiros escrito`:
//!
//! ```text
//! sbf roteiros escrito [--dados <pasta-de-dados>] <subcomando>
//!   importar <manifesto.json> --fonte <arquivo.luau> [--motivo <texto>]   (cria rascunho)
//!   listar | mostrar <nome> [--versao <n>]
//!   testar <nome> <versao> --cenario <cenario>   (roda no site falso; sai 0 passou, 1 falhou)
//!   ativar <nome> <versao> | desativar <nome>   (só em terminal interativo, com confirmação)
//!   executar <nome> --inventario <cofre> [--de M] [--ate M] [--forcar]   (roda a versão ativa)
//!   retomar <nome>   (só em terminal interativo: volta a agenda suspensa por entrada recusada)
//! ```
//!
//! O catálogo é o `roteiros.sqlite` da pasta de dados do app, fora do cofre. Ver [`AJUDA`].
//! `ativar` e `desativar` só rodam com stdin num terminal e depois de a pessoa digitar
//! exatamente o texto que o banco pede (`BancoDeRoteiros::esperado_para_ativar`):
//! `<nome>@<versao>` para o que o próprio usuário importou, e `<nome>@<versao> <host>` para o
//! que o MCP ou a IA propuseram (assim quem ativa é obrigado a ler o host que vai receber a
//! senha do chaveiro). `desativar` pede `<nome>`. O MCP e a IA, que não têm terminal, só
//! importam rascunhos. Nenhuma opção nem variável de ambiente pula isso.
//!
//! Toda string que veio do banco (autor, motivo, hosts, rótulos, fonte…) passa por um
//! escapador antes de ir ao terminal: controle e direção de texto viram `\xNN` ou `\u{..}`
//! visíveis, para que um rascunho hostil não mova o cursor, não apague linhas nem forje saída.
//!
//! `testar` é seguro e não pede terminal: sobe o site falso em `127.0.0.1`, abre o Chrome sem
//! janela num perfil descartável e usa senhas fictícias em memória (nunca o chaveiro real, o
//! Gmail, o perfil dedicado nem o cofre). Só o teste verde leva o rascunho a `testado`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_ia::{NivelDeConfianca, Registro};
use sbf_roteiros::banco::{AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoEscrito};
use sbf_roteiros::executor::Fim;
use sbf_roteiros::executor::FonteDeSenhas;
use sbf_roteiros::gravacao::Guia;
use sbf_roteiros::manifesto::{
    Classe, Manifesto, Mfa, escapar_fonte_para_terminal, escapar_para_terminal,
};
use sbf_roteiros::parametros::{EspecDeMes, ValorBruto};
use sbf_roteiros::servico::{
    Ambiente, PedidoDeExecucao, executar_devidos, executar_escrito_com, executar_escritos_devidos,
};
use sbf_roteiros::teste_escrito::{Cenario, testar_escrito};
use sbf_worker::laco::{ColetorDeRoteiros, Dados, ResumoDeRoteiros};
use sbf_worker::registro::Falha;

use crate::triagem::{Insumos, montar_motor};

/// Sem chaveiro aberto: os passos que pedem senha falham com um motivo claro.
struct SemChaveiro;

impl FonteDeSenhas for SemChaveiro {
    fn senha_de_site(&self, _rotulo: &str) -> Result<Segredo, String> {
        Err("o chaveiro não abriu sem a janela".to_owned())
    }
    fn cpf(&self) -> Result<Segredo, String> {
        Err("o chaveiro não abriu sem a janela".to_owned())
    }
}

/// Quanto o worker espera o usuário numa pausa: ele pode nem estar no computador.
const PAUSA_NO_WORKER: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// O ambiente dos roteiros para um cofre e uma pasta de dados.
#[must_use]
pub fn ambiente(cofre: &Path, pasta_de_dados: &Path) -> Ambiente {
    Ambiente::novo(pasta_de_dados, cofre)
}

/// Roda um trabalho demorado numa thread própria e devolve o resultado na passada seguinte:
/// um roteiro que espera o usuário (MFA, até 10 minutos) não pode segurar o laço do worker,
/// que tem coleta, triagem e índice para tocar.
pub struct ExecucaoEmFio {
    fio: Option<std::thread::JoinHandle<Result<ResumoDeRoteiros, Falha>>>,
    cancelar: std::sync::Arc<AtomicBool>,
}

impl ExecucaoEmFio {
    #[must_use]
    pub fn nova() -> Self {
        Self {
            fio: None,
            cancelar: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// Há um trabalho em andamento?
    #[must_use]
    pub fn rodando(&self) -> bool {
        self.fio.as_ref().is_some_and(|f| !f.is_finished())
    }

    /// Uma passada do laço. Nunca bloqueia: recolhe o resultado de um trabalho que terminou
    /// e, se nenhum está em andamento, inicia `trabalho` numa thread (o `cancelar` do
    /// trabalho acende quando o do worker acende). Devolve o resumo recolhido, ou vazio.
    ///
    /// # Errors
    /// A falha do trabalho que terminou.
    pub fn passar(
        &mut self,
        cancelar: &AtomicBool,
        trabalho: impl FnOnce(std::sync::Arc<AtomicBool>) -> Result<ResumoDeRoteiros, Falha>
        + Send
        + 'static,
    ) -> Result<ResumoDeRoteiros, Falha> {
        if cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            self.cancelar
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut recolhido = Ok(ResumoDeRoteiros::default());
        if let Some(f) = self.fio.take() {
            if f.is_finished() {
                recolhido = f.join().unwrap_or(Err(Falha::Externo));
            } else {
                self.fio = Some(f);
                return recolhido;
            }
        }
        if !self.cancelar.load(std::sync::atomic::Ordering::Relaxed) {
            let c = std::sync::Arc::clone(&self.cancelar);
            self.fio = Some(std::thread::spawn(move || trabalho(c)));
        }
        recolhido
    }
}

impl Default for ExecucaoEmFio {
    fn default() -> Self {
        Self::nova()
    }
}

impl Drop for ExecucaoEmFio {
    fn drop(&mut self) {
        // o worker saiu: o roteiro em curso é cancelado (a pausa e os passos olham o sinal)
        self.cancelar
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// O executor de roteiros agendados do worker. O roteiro roda numa thread própria: o laço
/// não espera MFA nem CAPTCHA.
pub struct RoteirosReais {
    dados: Dados,
    em_fio: ExecucaoEmFio,
}

impl RoteirosReais {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self {
            dados,
            em_fio: ExecucaoEmFio::nova(),
        }
    }
}

impl ColetorDeRoteiros for RoteirosReais {
    fn executar_devidos(
        &mut self,
        agora_: u64,
        cancelar: &AtomicBool,
    ) -> Result<ResumoDeRoteiros, Falha> {
        if self.em_fio.rodando() {
            return self
                .em_fio
                .passar(cancelar, |_| Ok(ResumoDeRoteiros::default()));
        }
        // Só abre o chaveiro e o motor se há o que rodar.
        let mut amb = ambiente(&self.dados.cofre, &self.dados.pasta);
        amb.prazo_da_pausa = PAUSA_NO_WORKER;
        let banco = amb.banco().map_err(|_| Falha::Banco)?;
        let agora_i = i64::try_from(agora_).unwrap_or(i64::MAX);
        // Os roteiros gravados (JSON) e os escritos (Luau) devidos pela agenda.
        let sem_gravados = banco.devidos(agora_i).map_err(|_| Falha::Banco)?.is_empty();
        let sem_escritos = banco
            .escritos_devidos(agora_i)
            .map_err(|_| Falha::Banco)?
            .is_empty();
        if sem_gravados && sem_escritos {
            return self
                .em_fio
                .passar(cancelar, |_| Ok(ResumoDeRoteiros::default()));
        }
        let dados = self.dados.clone();
        self.em_fio.passar(cancelar, move |cancelar| {
            rodar_devidos(&dados, &amb, agora_i, &cancelar)
        })
    }
}

/// O trabalho da thread: chaveiro, IA e a passada dos roteiros devidos.
fn rodar_devidos(
    dados: &Dados,
    amb: &Ambiente,
    agora_i: i64,
    cancelar: &AtomicBool,
) -> Result<ResumoDeRoteiros, Falha> {
    // O worker não pede senha: se o chaveiro não abre sozinho, os roteiros com senha falham.
    let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
    let senhas: &dyn FonteDeSenhas = match &chaveiro {
        Some(c) => c,
        None => &SemChaveiro,
    };
    let insumos = Insumos::do_ambiente(chaveiro.clone());
    let motor = montar_motor(&dados.banco, &insumos, true).ok();
    let registro: Option<&dyn Registro> = motor.as_ref().map(|m| &*m.registro as &dyn Registro);
    let guia = motor.as_ref().map(|m| Guia {
        provedor: &*m.principal,
        registro,
        destinatario: "roteiros".to_owned(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: chaveiro
            .as_ref()
            .map(Chaveiro::rotulos_de_site)
            .unwrap_or_default(),
        tem_cpf: chaveiro.as_ref().is_some_and(Chaveiro::tem_cpf),
        segredos: Vec::new(),
    });
    let r = executar_devidos(amb, agora_i, senhas, guia.as_ref(), cancelar)
        .map_err(|_| Falha::Externo)?;
    // Os roteiros escritos (Luau) seguem a agenda do manifesto, um de cada vez, depois dos
    // gravados. A falha de um não impede o outro; o Chrome abre e fecha em cada execução.
    let e =
        executar_escritos_devidos(amb, agora_i, senhas, cancelar).map_err(|_| Falha::Externo)?;
    Ok(ResumoDeRoteiros {
        executados: r.executados + e.executados,
        baixados: (r.baixados + e.baixados) as usize,
        novos: (r.novos + e.novos) as usize,
    })
}

/// A IA dos roteiros na janela: o mesmo motor da triagem (o `claude` do usuário como
/// principal), montado quando a gravação ou o conserto pedem.
pub struct IaDoInventario {
    pub banco: std::path::PathBuf,
    pub insumos: Insumos,
}

impl sbf_janela::roteiros::IaDosRoteiros for IaDoInventario {
    fn pronta(&self) -> Result<sbf_janela::roteiros::ProvedorEmRegistro, String> {
        let motor = montar_motor(&self.banco, &self.insumos, true)?;
        Ok((motor.principal, motor.registro))
    }
}

// ---------------------------------------------------------------------------------------
// `sbf roteiros escrito`: o ciclo de vida do roteiro escrito (Luau) pelo terminal
// ---------------------------------------------------------------------------------------

pub const AJUDA: &str = "\
uso: sbf roteiros escrito [--dados <pasta-de-dados>] <subcomando>

  importar <manifesto.json> --fonte <arquivo.luau> [--motivo <texto>]
                                         cria um RASCUNHO (nunca ativa); o mesmo conteúdo
                                         da versão mais nova não duplica
  listar                                 os roteiros escritos, a versão em vigor e a mais nova
  mostrar <nome> [--versao <n>]          manifesto, fonte, versões e histórico de estados
  testar <nome> <versao> --cenario <c>   roda a versão no site falso local (Chrome sem janela,
                                         senhas fictícias); passou => `testado`. Sai 0 se passou,
                                         1 se falhou. Cenários: contracheque, conexoes, soucaixa
  ativar <nome> <versao>                 só num terminal interativo, com confirmação digitada;
                                         só de uma versão testada; aposenta a ativa anterior.
                                         Mostra autor, hosts, rótulos do chaveiro, MFA e o fonte
                                         Luau; se foi proposta pelo MCP ou pela IA, a digitação
                                         é `<nome>@<versao> <host>`
  desativar <nome>                       idem (aposenta a versão ativa)
  executar <nome> --inventario <pasta-do-cofre> [--de <mês>] [--ate <mês>] [--forcar]
                                         roda AGORA a versão ativa, no Chrome do perfil dedicado e
                                         com o chaveiro real. `--de` e `--ate` entregam os
                                         parâmetros de mês que o manifesto declara (`AAAA-MM` ou
                                         relativo ao mês corrente: `0` este mês, `-1` o anterior);
                                         sem eles valem os padrões do manifesto. `--forcar` é a
                                         Baixa forçada (baixa de novo o que o inventário já tem):
                                         só num terminal interativo. Se o roteiro pedir MFA ou
                                         CAPTCHA, resolva no navegador; funciona mesmo com a
                                         agenda suspensa e, se der certo, NÃO a retoma
  retomar <nome>                         só num terminal interativo: volta a agenda do roteiro
                                         que parou em `entrada_recusada` (o portal recusou a
                                         entrada). O MCP e a IA nunca retomam
  entrar --url <https://...>             login à mão, SEM controle: abre o Chrome do perfil dedicado
                                         sem a porta de automação (sites como o Google recusam
                                         navegador controlado). Entre com Google, e-mail, o que
                                         for; feche a janela ao terminar. A sessão do site fica no
                                         perfil para os roteiros. O SBF não lê nem digita nada
  observar --url <https://...> [--saida <pasta>] [--minutos <n>] [--intervalo-ms <n>]
                                         modo ASSISTIDO: abre o Chrome visível com o perfil
                                         dedicado e só LÊ. Você navega e entra à mão; a cada
                                         mudança grava a página simplificada (sem valor de
                                         campo) em `pagina-NNN.txt`, para quem for escrever o
                                         roteiro ler. Não clica nem digita. Termina no prazo
                                         (padrão 20 min, máx 60) ou ao criar o arquivo `parar`
                                         na pasta de saída (padrão: <dados>/roteiros/observacao)

O roteiro escrito vive em `roteiros.sqlite`, na pasta de dados do app (fora do cofre).
Ativar e desativar não rodam sem terminal: o MCP e a IA só propõem (criam rascunho).";

/// O que dá errado, já em português para o terminal.
#[derive(Debug, PartialEq, Eq)]
enum FalhaDoCli {
    /// Uso errado da linha de comando (código 2).
    Uso(String),
    /// A operação falhou (código 1).
    Operacao(String),
}

fn uso(m: impl Into<String>) -> FalhaDoCli {
    FalhaDoCli::Uso(m.into())
}

fn op(m: impl Into<String>) -> FalhaDoCli {
    FalhaDoCli::Operacao(m.into())
}

impl From<sbf_roteiros::ErroDeRoteiro> for FalhaDoCli {
    fn from(e: sbf_roteiros::ErroDeRoteiro) -> Self {
        Self::Operacao(e.to_string())
    }
}

/// O maior arquivo lido (manifesto ou fonte): 64 KiB, como o motor.
const LIMITE_DO_ARQUIVO: u64 = 64 * 1024;

/// Onde o CLI roda: a pasta de dados e o terminal.
struct AmbienteDoCli<'a> {
    dados: &'a Path,
    /// O Chrome a usar no `testar` (`None`: o que for achado; só os testes trocam).
    chrome: Option<PathBuf>,
    terminal: bool,
    entrada: &'a mut dyn std::io::BufRead,
    saida: &'a mut dyn std::io::Write,
}

/// Escreve uma linha (ou o fonte, de várias). Rede de segurança: nenhum controle (fora `\n` e
/// tabulação) nem direção de texto chega ao terminal, mesmo que um campo tenha escapado do
/// escapador específico dele.
fn w(saida: &mut dyn std::io::Write, linha: &str) {
    let _ = writeln!(saida, "{}", escapar_fonte_para_terminal(linha));
}

fn ler_texto(caminho: &Path) -> Result<String, FalhaDoCli> {
    let meta = std::fs::metadata(caminho)
        .map_err(|e| op(format!("não deu para ler {}: {e}", caminho.display())))?;
    if !meta.is_file() {
        return Err(op(format!("{} não é um arquivo", caminho.display())));
    }
    if meta.len() > LIMITE_DO_ARQUIVO {
        return Err(op(format!(
            "{} passa de 64 KiB; o motor não lê arquivos desse tamanho",
            caminho.display()
        )));
    }
    std::fs::read_to_string(caminho).map_err(|e| {
        op(format!(
            "não deu para ler {} como texto (UTF-8): {e}",
            caminho.display()
        ))
    })
}

fn abrir_banco(a: &AmbienteDoCli) -> Result<BancoDeRoteiros, FalhaDoCli> {
    Ok(BancoDeRoteiros::abrir(&a.dados.join("roteiros.sqlite"))?)
}

fn versao_de(texto: &str) -> Result<u32, FalhaDoCli> {
    texto.parse::<u32>().ok().filter(|v| *v > 0).ok_or_else(|| {
        uso(format!(
            "versão inválida `{texto}` (um número a partir de 1)"
        ))
    })
}

fn curto(checksum: &str) -> &str {
    checksum.get(..12).unwrap_or(checksum)
}

/// Uma string do banco ou do manifesto, escapada para ir ao terminal (uma linha só).
fn e(texto: &str) -> String {
    escapar_para_terminal(texto)
}

fn mfa_texto(m: &Mfa) -> String {
    match m {
        Mfa::Nenhum => "nenhum".to_owned(),
        Mfa::Manual => {
            "manual (código do aplicativo autenticador, CAPTCHA: o roteiro pausa e a vez é sua)"
                .to_owned()
        }
        Mfa::Email { remetente, formato } => format!(
            "código por e-mail de {}, formato {} (o roteiro lê o seu Gmail)",
            e(remetente),
            e(formato)
        ),
    }
}

fn resumo_do_manifesto(m: &Manifesto) -> Vec<String> {
    let hosts: Vec<String> = m.hosts.iter().map(|h| e(h)).collect();
    let mut linhas = vec![
        format!("  hosts: {}", hosts.join(", ")),
        format!("  url inicial: {}", e(&m.url_inicial)),
        format!("  mfa: {}", mfa_texto(&m.mfa)),
        format!(
            "  movimenta_dinheiro: {} (declarado pelo autor, não verificado)",
            m.movimenta_dinheiro
        ),
        format!(
            "  classe: {}",
            match m.classe {
                Classe::Consulta => "consulta",
                Classe::AtualizarConexao => "atualizar conexão",
            }
        ),
        format!(
            "  agenda: {}",
            if m.agenda.is_empty() {
                m.agenda_horas
                    .map_or_else(|| "sem agenda".to_owned(), |h| format!("a cada {h} h"))
            } else {
                "por horário (fuso UTC−3), roda uma vez ao voltar se perdeu o horário".to_owned()
            }
        ),
        format!(
            "  chaveiro: senha {}, CPF {}",
            m.rotulo_senha
                .as_deref()
                .map_or_else(|| "não usa".to_owned(), e),
            m.rotulo_cpf
                .as_deref()
                .map_or_else(|| "não usa".to_owned(), e)
        ),
    ];
    linhas.extend(linhas_da_agenda(m));
    linhas
}

/// Cada entrada da agenda e cada parâmetro declarado, uma linha por item.
fn linhas_da_agenda(m: &Manifesto) -> Vec<String> {
    let mut l = Vec::new();
    for (i, entrada) in m.agenda.iter().enumerate() {
        let horarios: Vec<String> = entrada.horarios.iter().map(|h| e(h)).collect();
        let dias = entrada.dias_do_mes.as_ref().map_or_else(
            || "todo dia".to_owned(),
            |d| {
                let d: Vec<String> = d.iter().map(ToString::to_string).collect();
                format!("nos dias {}", d.join(", "))
            },
        );
        let parametros: Vec<String> = entrada
            .parametros
            .iter()
            .map(|(n, v)| format!("{}={}", e(n), valor_bruto_texto(v)))
            .collect();
        l.push(format!(
            "    entrada {}: {} {dias}{}",
            i + 1,
            horarios.join(", "),
            if parametros.is_empty() {
                String::new()
            } else {
                format!(" ({})", parametros.join(", "))
            }
        ));
    }
    for p in &m.parametros {
        l.push(format!(
            "    parâmetro {}: {}{}",
            e(&p.nome),
            match p.tipo {
                sbf_roteiros::parametros::TipoDeParametro::Mes => "mês",
                sbf_roteiros::parametros::TipoDeParametro::Booleano => "sim ou não",
            },
            p.padrao.as_ref().map_or(String::new(), |v| format!(
                ", padrão {}",
                valor_bruto_texto(v)
            ))
        ));
    }
    l
}

fn valor_bruto_texto(v: &ValorBruto) -> String {
    match v {
        ValorBruto::Texto(t) => e(t),
        ValorBruto::Booleano(b) => b.to_string(),
    }
}

/// Quantas linhas do fonte o `ativar` imprime inteiras; acima disso manda ler com `mostrar`.
const LINHAS_DO_FONTE_NO_ATIVAR: usize = 200;

/// O que o `ativar` mostra em destaque antes da digitação: de quem é a versão e para onde vai
/// a senha do chaveiro. O autor, os hosts e o rótulo são **autodeclarados** pelo autor da
/// versão; quando o autor não é o próprio usuário, o aviso diz isso com todas as letras.
fn destaque_do_ativar(v: &sbf_roteiros::banco::VersaoEscrita) -> Vec<String> {
    let m = &v.manifesto;
    let autor = e(&v.autor.texto());
    let hosts: Vec<String> = m.hosts.iter().map(|h| e(h)).collect();
    let mut l = vec![
        "==================== CONFIRA ANTES DE ATIVAR ====================".to_owned(),
        format!("  AUTOR DA VERSÃO: {autor}"),
        format!(
            "  HOSTS (os únicos que o roteiro abre e que recebem a senha): {}",
            hosts.join(", ")
        ),
        format!(
            "  RÓTULO DA SENHA no chaveiro: {}",
            m.rotulo_senha
                .as_deref()
                .map_or_else(|| "não usa".to_owned(), e)
        ),
        format!(
            "  RÓTULO DO CPF no chaveiro: {}",
            m.rotulo_cpf
                .as_deref()
                .map_or_else(|| "não usa".to_owned(), e)
        ),
        format!(
            "  MFA POR E-MAIL: {}",
            match &m.mfa {
                Mfa::Email { remetente, .. } => format!(
                    "SIM, o roteiro vai ler o seu Gmail atrás do código de {}",
                    e(remetente)
                ),
                Mfa::Manual => "não (MFA manual: o roteiro pausa e a vez é sua)".to_owned(),
                Mfa::Nenhum => "não".to_owned(),
            }
        ),
        format!(
            "  movimenta_dinheiro: {} (declarado pelo autor, não verificado)",
            m.movimenta_dinheiro
        ),
    ];
    if v.autor != AutorEscrito::Usuario {
        l.push(format!(
            "  !! PROPOSTO POR {autor}: não foi você quem escreveu. Os hosts e o rótulo da senha acima são declarados por esse autor: a senha do chaveiro `{}` será digitada em {}. Só ative se esse host é mesmo o site daquele rótulo.",
            m.rotulo_senha.as_deref().map_or_else(|| "(nenhuma)".to_owned(), e),
            hosts.first().map_or("", String::as_str)
        ));
    }
    l.push("=================================================================".to_owned());
    l
}

/// Opções `--nome valor` e argumentos soltos (sem repetição, sem desconhecidas).
fn opcoes(
    resto: &[String],
    com_valor: &[&str],
) -> Result<(Vec<String>, std::collections::BTreeMap<String, String>), FalhaDoCli> {
    let mut soltos = Vec::new();
    let mut valores = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < resto.len() {
        let a = resto[i].as_str();
        if com_valor.contains(&a) {
            let v = resto
                .get(i + 1)
                .ok_or_else(|| uso(format!("{a} pede um valor")))?;
            if valores.insert(a.to_owned(), v.clone()).is_some() {
                return Err(uso(format!("{a} repetido")));
            }
            i += 1;
        } else if a.starts_with("--") {
            return Err(uso(format!("argumento desconhecido: {a}")));
        } else {
            soltos.push(a.to_owned());
        }
        i += 1;
    }
    Ok((soltos, valores))
}

fn importar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let (soltos, valores) = opcoes(resto, &["--fonte", "--motivo"])?;
    let [arquivo] = soltos.as_slice() else {
        return Err(uso(
            "uso: importar <manifesto.json> --fonte <arquivo.luau> [--motivo <texto>]",
        ));
    };
    let caminho_do_fonte = valores
        .get("--fonte")
        .ok_or_else(|| uso("importar pede --fonte <arquivo.luau>"))?;
    let manifesto = Manifesto::de_json(&ler_texto(Path::new(arquivo))?)?;
    // Fim de linha do Windows (CRLF) vira LF: o banco recusa `\r` solto (é como se esconde
    // texto no terminal), então só o par CRLF é aceito aqui e normalizado.
    let fonte = ler_texto(Path::new(caminho_do_fonte))?.replace("\r\n", "\n");
    let motivo = valores.get("--motivo").map_or("", String::as_str);
    let banco = abrir_banco(a)?;
    let i = banco.importar_escrito(&manifesto, &fonte, &AutorEscrito::Usuario, motivo)?;
    if i.criada {
        w(
            a.saida,
            &format!(
                "{} v{}: rascunho criado. Falta testar no site falso e ativar num terminal interativo.",
                e(&manifesto.nome),
                i.numero
            ),
        );
    } else {
        w(
            a.saida,
            &format!(
                "{} v{}: já é a versão mais nova com este conteúdo; nada foi criado.",
                e(&manifesto.nome),
                i.numero
            ),
        );
    }
    Ok(())
}

fn listar(a: &mut AmbienteDoCli) -> Result<(), FalhaDoCli> {
    let banco = abrir_banco(a)?;
    let linhas = banco.listar_escritos()?;
    if linhas.is_empty() {
        w(a.saida, "Nenhum roteiro escrito.");
        return Ok(());
    }
    for l in linhas {
        let em_vigor = l.ativa.map_or_else(
            || "nenhuma versão em vigor".to_owned(),
            |v| format!("v{v} em vigor"),
        );
        let suspensa = if banco.agenda_suspensa_escrita(&l.nome)?.is_some() {
            " [agenda SUSPENSA: veja `mostrar`; retome com `retomar`]"
        } else {
            ""
        };
        w(
            a.saida,
            &format!(
                "{}: {em_vigor}; mais nova v{} ({}){suspensa}",
                e(&l.nome),
                l.ultima,
                l.estado_da_ultima.texto()
            ),
        );
    }
    Ok(())
}

fn mostrar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let (soltos, valores) = opcoes(resto, &["--versao"])?;
    let [nome] = soltos.as_slice() else {
        return Err(uso("uso: mostrar <nome> [--versao <n>]"));
    };
    let banco = abrir_banco(a)?;
    let d = banco.mostrar(nome)?;
    let escolhida = match valores.get("--versao") {
        Some(v) => versao_de(v)?,
        None => d
            .versoes
            .iter()
            .find(|v| v.estado == EstadoEscrito::Ativo)
            .or_else(|| d.versoes.last())
            .map_or(0, |v| v.numero),
    };
    w(a.saida, &format!("{}:", e(&d.nome)));
    for v in &d.versoes {
        w(
            a.saida,
            &format!(
                "  v{}: {} (autor {}, checksum {}, {}){}{}",
                v.numero,
                v.estado.texto(),
                e(&v.autor.texto()),
                e(curto(&v.checksum)),
                e(&v.criada_em),
                if v.motivo.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", e(&v.motivo))
                },
                v.resultado_do_teste
                    .as_ref()
                    .map_or(String::new(), |t| format!(" [teste {}]", e(t))),
            ),
        );
    }
    let v = d
        .versoes
        .iter()
        .find(|v| v.numero == escolhida)
        .ok_or_else(|| op(format!("`{}` não tem a versão {escolhida}", e(nome))))?;
    if let Some(sp) = banco.agenda_suspensa_escrita(nome)? {
        w(
            a.saida,
            &format!(
                "AGENDA SUSPENSA (entrada recusada pelo portal): {}. Para voltar: sbf roteiros escrito retomar {}",
                e(&sp.motivo),
                e(nome)
            ),
        );
    }
    w(a.saida, &format!("Manifesto da v{}:", v.numero));
    if !v.manifesto.descricao.is_empty() {
        w(a.saida, &format!("  {}", e(&v.manifesto.descricao)));
    }
    for l in resumo_do_manifesto(&v.manifesto) {
        w(a.saida, &l);
    }
    w(a.saida, &format!("Fonte Luau da v{}:", v.numero));
    w(a.saida, &escapar_fonte_para_terminal(&v.fonte));
    w(a.saida, "Histórico:");
    for (quando, versao, de, para, quem, detalhe) in &d.eventos {
        w(
            a.saida,
            &format!(
                "  {} v{versao}: {} -> {} ({}){}",
                e(quando),
                e(de.as_deref().unwrap_or("novo")),
                e(para),
                e(quem),
                if detalhe.is_empty() {
                    String::new()
                } else {
                    format!(" {}", e(detalhe))
                }
            ),
        );
    }
    Ok(())
}

fn recusa_sem_terminal(verbo: &str) -> FalhaDoCli {
    op(format!(
        "{verbo} só na janela ou num terminal interativo; o MCP e a IA não ativam. Nada foi alterado."
    ))
}

/// Mostra o resumo, pede a digitação exata e devolve a confirmação. Sem terminal, nem pergunta.
fn confirmar(
    a: &mut AmbienteDoCli,
    resumo: &[String],
    esperado: &str,
) -> Result<Confirmacao, FalhaDoCli> {
    for l in resumo {
        w(a.saida, l);
    }
    let _ = write!(
        a.saida,
        "Para confirmar, digite exatamente `{}`: ",
        e(esperado)
    );
    let _ = a.saida.flush();
    let mut linha = String::new();
    let lidos = a
        .entrada
        .read_line(&mut linha)
        .map_err(|e| op(format!("não deu para ler a confirmação: {e}")))?;
    let digitado = if lidos == 0 {
        ""
    } else {
        linha.trim_end_matches(['\n', '\r'])
    };
    Ok(Confirmacao::do_terminal(a.terminal, digitado, esperado)?)
}

fn ativar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let [nome, versao] = resto else {
        return Err(uso("uso: ativar <nome> <versao>"));
    };
    let versao = versao_de(versao)?;
    // O terminal vem primeiro: sem ele nem o banco é aberto.
    if !a.terminal {
        return Err(recusa_sem_terminal("ativar"));
    }
    let banco = abrir_banco(a)?;
    let v = banco.versao_escrita(nome, versao)?;
    if v.estado != EstadoEscrito::Testado {
        return Err(op(format!(
            "só uma versão testada pode ser ativada; `{}` v{versao} está {}",
            e(nome),
            v.estado.texto()
        )));
    }
    let mut resumo = vec![format!(
        "ativar `{}` v{versao} (checksum {}){}",
        e(nome),
        e(&v.checksum),
        if v.motivo.is_empty() {
            String::new()
        } else {
            format!(": {}", e(&v.motivo))
        }
    )];
    resumo.extend(destaque_do_ativar(&v));
    if !v.manifesto.descricao.is_empty() {
        resumo.push(format!("  descrição: {}", e(&v.manifesto.descricao)));
    }
    resumo.extend(resumo_do_manifesto(&v.manifesto));
    let linhas = v.fonte.lines().count();
    if linhas <= LINHAS_DO_FONTE_NO_ATIVAR {
        resumo.push(format!(
            "Fonte Luau ({linhas} linhas), leia antes de digitar:"
        ));
        resumo.push(escapar_fonte_para_terminal(&v.fonte));
        resumo.push("--- fim do fonte ---".to_owned());
    } else {
        resumo.push(format!(
            "Fonte Luau: {linhas} linhas (passa de {LINHAS_DO_FONTE_NO_ATIVAR}), checksum {}. Leia antes de digitar: sbf roteiros escrito mostrar {} --versao {versao}",
            e(&v.checksum),
            e(nome)
        ));
    }
    // A única fonte do texto a digitar é o banco (inclui o host se o autor não é o usuário).
    let esperado = banco.esperado_para_ativar(nome, versao)?;
    let conf = confirmar(a, &resumo, &esperado)?;
    banco.ativar(nome, versao, &conf)?;
    w(a.saida, &format!("{} v{versao}: ativada.", e(nome)));
    Ok(())
}

fn desativar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let [nome] = resto else {
        return Err(uso("uso: desativar <nome>"));
    };
    if !a.terminal {
        return Err(recusa_sem_terminal("desativar"));
    }
    let banco = abrir_banco(a)?;
    let Some(v) = banco.versao_ativa_escrita(nome)? else {
        return Err(op(format!("`{}` não tem versão ativa", e(nome))));
    };
    let resumo = vec![format!(
        "desativar `{}` (v{} em vigor, autor {})",
        e(nome),
        v.numero,
        e(&v.autor.texto())
    )];
    let conf = confirmar(a, &resumo, &banco.esperado_para_desativar(nome))?;
    let n = banco.desativar(nome, &conf)?;
    w(a.saida, &format!("{} v{n}: desativada.", e(nome)));
    Ok(())
}

fn cenarios_disponiveis(a: &mut AmbienteDoCli) {
    w(a.saida, "Cenários disponíveis:");
    for c in Cenario::TODOS {
        w(a.saida, &format!("  {:<14} {}", c.nome(), c.descricao()));
    }
}

fn testar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let (soltos, valores) = opcoes(resto, &["--cenario"])?;
    let [nome, versao] = soltos.as_slice() else {
        return Err(uso("uso: testar <nome> <versao> --cenario <cenario>"));
    };
    let versao = versao_de(versao)?;
    let Some(pedido) = valores.get("--cenario") else {
        cenarios_disponiveis(a);
        return Err(uso("testar pede --cenario <cenario>"));
    };
    let Some(cenario) = Cenario::de_texto(pedido) else {
        cenarios_disponiveis(a);
        return Err(uso(format!("cenário desconhecido: `{pedido}`")));
    };
    // Só o banco dos roteiros é o real; o cofre, o perfil do Chrome e as senhas do teste são
    // descartáveis e fictícios (o `testar_escrito` cria e apaga). O cofre daqui nunca é aberto.
    let mut amb = Ambiente::novo(a.dados, &a.dados.join("cofre-do-teste-nao-usado"));
    amb.programa_do_chrome.clone_from(&a.chrome);
    w(
        a.saida,
        &format!(
            "Testando `{}` v{versao} no cenário {} (site falso em 127.0.0.1, Chrome sem janela)...",
            e(nome),
            cenario.nome()
        ),
    );
    let r = testar_escrito(&amb, nome, versao, cenario)?;
    if r.passou {
        w(
            a.saida,
            &format!("{} v{versao}: PASSOU. {}", e(nome), e(&r.resumo)),
        );
        w(
            a.saida,
            &format!(
                "A versão está `testado`. Para ativar, num terminal: sbf roteiros escrito ativar {} {versao}",
                e(nome)
            ),
        );
        Ok(())
    } else {
        w(
            a.saida,
            &format!("{} v{versao}: FALHOU. {}", e(nome), e(&r.resumo)),
        );
        Err(op(format!(
            "o teste de `{}` v{versao} falhou; a versão continua rascunho",
            e(nome)
        )))
    }
}

/// O prazo padrão e o máximo do `observar`, em minutos.
const OBSERVAR_PADRAO_MIN: u64 = 20;
const OBSERVAR_MAXIMO_MIN: u64 = 60;

/// `observar`: o modo assistido. Só lê (nunca clica nem digita); quem navega e entra é o
/// usuário, no Chrome visível do perfil dedicado, e os cookies ficam nesse perfil para os
/// roteiros. A página simplificada (sem valor de campo, número mascarado) vai para arquivos.
fn observar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    use sbf_roteiros::Navegador as _;
    use std::hash::{DefaultHasher, Hash as _, Hasher as _};
    let (soltos, valores) = opcoes(resto, &["--url", "--saida", "--minutos", "--intervalo-ms"])?;
    if !soltos.is_empty() {
        return Err(uso(
            "uso: observar --url <https://...> [--saida <pasta>] [--minutos <n>]",
        ));
    }
    let url = valores
        .get("--url")
        .ok_or_else(|| uso("observar pede --url <https://...>"))?;
    if !url.starts_with("https://") || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(uso("a URL do observar tem de ser https:// e sem espaços"));
    }
    let minutos = match valores.get("--minutos") {
        None => OBSERVAR_PADRAO_MIN,
        Some(t) => t
            .parse::<u64>()
            .ok()
            .filter(|m| (1..=OBSERVAR_MAXIMO_MIN).contains(m))
            .ok_or_else(|| uso(format!("--minutos vai de 1 a {OBSERVAR_MAXIMO_MIN}")))?,
    };
    let intervalo = match valores.get("--intervalo-ms") {
        None => 2000,
        Some(t) => t
            .parse::<u64>()
            .ok()
            .filter(|m| (250..=10_000).contains(m))
            .ok_or_else(|| uso("--intervalo-ms vai de 250 a 10000"))?,
    };
    let saida = valores.get("--saida").map_or_else(
        || a.dados.join("roteiros").join("observacao"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&saida)
        .map_err(|e| op(format!("não deu para criar {}: {e}", saida.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&saida, std::fs::Permissions::from_mode(0o700));
    }
    let parar = saida.join("parar");
    let _ = std::fs::remove_file(&parar);

    let mut amb = Ambiente::novo(a.dados, &a.dados.join("cofre-do-observar-nao-usado"));
    amb.programa_do_chrome.clone_from(&a.chrome);
    let mut chrome = amb.abrir_chrome()?;
    chrome
        .navegar(url)
        .map_err(|e| op(format!("não abriu a página: {e}")))?;
    w(
        a.saida,
        &format!(
            "Observando por até {minutos} min (só leitura). Páginas em {}. Para encerrar antes: crie {}",
            saida.display(),
            parar.display()
        ),
    );
    let fim = std::time::Instant::now() + std::time::Duration::from_secs(minutos * 60);
    let mut ultimo: u64 = 0;
    let mut n: u32 = 0;
    // Sondas: seletores comuns de diálogo/iframe/progresso (o conteúdo de um iframe de outro
    // domínio a página simplificada não enxerga). Só registra quando o conjunto visível muda.
    const SONDAS: [&str; 9] = [
        "iframe",
        "iframe[src*=\"pluggy\"]",
        "[role=\"dialog\"]",
        "[role=\"alertdialog\"]",
        "[role=\"progressbar\"]",
        "dialog",
        "[aria-modal=\"true\"]",
        "[class*=\"modal\"]",
        "[class*=\"overlay\"]",
    ];
    let inicio = std::time::Instant::now();
    let mut sondas_antes = String::new();
    while std::time::Instant::now() < fim && !parar.exists() {
        let visiveis: Vec<&str> = SONDAS
            .iter()
            .copied()
            .filter(|s| chrome.visivel(s).unwrap_or(false))
            .collect();
        let linha = visiveis.join(" ; ");
        if linha != sondas_antes {
            sondas_antes.clone_from(&linha);
            let registro = format!("{:>6.1}s  {}\n", inicio.elapsed().as_secs_f32(), linha);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(saida.join("sondas.txt"))
                .and_then(|mut f| std::io::Write::write_all(&mut f, registro.as_bytes()));
        }
        // Uma leitura que falha (a página está navegando) não encerra: tenta de novo.
        if let Ok(pagina) = chrome.pagina() {
            let texto = pagina.mascarar().para_texto();
            let mut h = DefaultHasher::new();
            texto.hash(&mut h);
            let marca = h.finish();
            if marca != ultimo {
                ultimo = marca;
                n += 1;
                let arquivo = saida.join(format!("pagina-{n:03}.txt"));
                std::fs::write(&arquivo, &texto)
                    .map_err(|e| op(format!("não deu para gravar {}: {e}", arquivo.display())))?;
                w(a.saida, &format!("pagina-{n:03}.txt gravada"));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(intervalo));
    }
    chrome.fechar();
    w(
        a.saida,
        &format!("Observação encerrada: {n} página(s) gravada(s)."),
    );
    Ok(())
}

/// `entrar`: o login assistido SEM controle. O Chrome sobe sem porta de depuração (nada de CDP),
/// no mesmo perfil dedicado dos roteiros, e o SBF só espera a janela fechar. É para sites que
/// recusam navegador controlado (login com Google): o que fica no perfil é a sessão do próprio
/// site, que os roteiros reaproveitam depois.
fn entrar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    use sbf_roteiros::chrome::{
        Lancador as _, LancadorDoSbf, TravaDoPerfil, apagar_dados_de_formulario,
        argumentos_do_chrome, localizar_chrome, preparar_preferencias,
    };
    let (soltos, valores) = opcoes(resto, &["--url"])?;
    if !soltos.is_empty() {
        return Err(uso("uso: entrar --url <https://...>"));
    }
    let url = valores
        .get("--url")
        .ok_or_else(|| uso("entrar pede --url <https://...>"))?;
    if !url.starts_with("https://") || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(uso("a URL do entrar tem de ser https:// e sem espaços"));
    }
    let amb = Ambiente::novo(a.dados, &a.dados.join("cofre-do-entrar-nao-usado"));
    let perfil = amb.perfil();
    let programa = a
        .chrome
        .clone()
        .or_else(localizar_chrome)
        .ok_or_else(|| op("não achei o Chrome nem o Chromium instalado"))?;
    std::fs::create_dir_all(&perfil)
        .map_err(|e| op(format!("não deu para criar o perfil: {e}")))?;
    let _trava = TravaDoPerfil::obter(&perfil).map_err(|e| op(e.to_string()))?;
    apagar_dados_de_formulario(&perfil);
    preparar_preferencias(&perfil).map_err(|e| op(e.to_string()))?;
    let mut args = argumentos_do_chrome(&perfil, false, LancadorDoSbf.sem_sandbox());
    // Sem controle: fora a porta de depuração; a página inicial é a pedida.
    args.retain(|x| !x.starts_with("--remote-debugging-port") && x != "about:blank");
    args.push(url.clone());
    let mut filho = LancadorDoSbf.lancar(&programa, &args).map_err(op)?;
    w(
        a.saida,
        "Chrome aberto SEM controle. Entre no site; ao terminar, feche a janela do Chrome.",
    );
    filho
        .wait()
        .map_err(|e| op(format!("falha ao esperar o Chrome: {e}")))?;
    w(
        a.saida,
        "Chrome fechado; a sessão ficou no perfil dedicado.",
    );
    Ok(())
}

/// O que o `executar` entende da linha de comando.
#[derive(Debug, PartialEq, Eq)]
struct ExecucaoPedida {
    nome: String,
    cofre: PathBuf,
    pedido: PedidoDeExecucao,
}

/// Monta o pedido de `executar <nome> --inventario <cofre> [--de M] [--ate M] [--forcar]`.
/// Só confere a forma (mês legível, `--forcar` só com terminal); o manifesto confere o resto
/// (nome declarado, de ≤ ate, 60 meses) na hora de resolver.
fn montar_execucao(resto: &[String], terminal: bool) -> Result<ExecucaoPedida, FalhaDoCli> {
    let mut sem_flag: Vec<String> = Vec::new();
    let mut forcar = false;
    for a in resto {
        if a == "--forcar" {
            if forcar {
                return Err(uso("--forcar repetido"));
            }
            forcar = true;
        } else {
            sem_flag.push(a.clone());
        }
    }
    let (soltos, valores) = opcoes(&sem_flag, &["--de", "--ate", "--inventario"])?;
    let [nome] = soltos.as_slice() else {
        return Err(uso(
            "uso: executar <nome> --inventario <pasta-do-cofre> [--de <mês>] [--ate <mês>] [--forcar]",
        ));
    };
    let cofre = valores.get("--inventario").ok_or_else(|| {
        uso("executar pede --inventario <pasta-do-cofre> (onde caem os downloads)")
    })?;
    let mut pedido = PedidoDeExecucao {
        forcar,
        ..PedidoDeExecucao::default()
    };
    for opcao in ["--de", "--ate"] {
        if let Some(v) = valores.get(opcao) {
            if EspecDeMes::ler(v).is_none() {
                return Err(uso(format!(
                    "{opcao} pede um mês: AAAA-MM (como 2026-11) ou relativo ao corrente (0, -1, -12)"
                )));
            }
            pedido.entregues.insert(
                opcao.trim_start_matches("--").to_owned(),
                ValorBruto::Texto(v.clone()),
            );
        }
    }
    if forcar && !terminal {
        return Err(op(
            "--forcar só num terminal interativo; o MCP e a IA só propõem. Nada foi executado.",
        ));
    }
    Ok(ExecucaoPedida {
        nome: nome.clone(),
        cofre: PathBuf::from(cofre),
        pedido,
    })
}

/// `executar`: roda a versão ativa agora, no Chrome e com o chaveiro de verdade.
fn executar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let pedida = montar_execucao(resto, a.terminal)?;
    let chaveiro = Chaveiro::abrir_na_pasta_padrao().ok();
    let senhas: &dyn FonteDeSenhas = match &chaveiro {
        Some(c) => c,
        None => &SemChaveiro,
    };
    let mut amb = Ambiente::novo(a.dados, &pedida.cofre);
    amb.programa_do_chrome.clone_from(&a.chrome);
    w(
        a.saida,
        &format!(
            "Executando `{}`{} (Chrome do perfil dedicado, chaveiro real)...",
            e(&pedida.nome),
            if pedida.pedido.forcar {
                " com Baixa forçada"
            } else {
                ""
            }
        ),
    );
    let cancelar = AtomicBool::new(false);
    let rel = executar_escrito_com(
        &amb,
        &pedida.nome,
        "terminal",
        senhas,
        &cancelar,
        &pedida.pedido,
    )?;
    w(
        a.saida,
        &format!(
            "{} baixado(s), {} novo(s), {} descartado(s) por já estarem no inventário.",
            rel.baixados, rel.novos, rel.descartados
        ),
    );
    if let Some(motivo) = &rel.entrada_recusada {
        w(
            a.saida,
            &format!(
                "O portal recusou a entrada: {}. A agenda de `{}` foi suspensa; depois de resolver, rode: sbf roteiros escrito retomar {}",
                e(motivo),
                e(&pedida.nome),
                e(&pedida.nome)
            ),
        );
        return Err(op("entrada recusada"));
    }
    match rel.fim {
        Fim::Concluida => {
            w(a.saida, "Concluída.");
            Ok(())
        }
        Fim::Falhou { motivo, .. } => Err(op(format!("o roteiro falhou: {motivo}"))),
        Fim::Bloqueada { bloqueio, .. } => {
            Err(op(format!("barrado pela lista de bloqueio: {bloqueio}")))
        }
        Fim::Cancelada => Err(op("cancelada")),
        Fim::PausaExpirou { .. } => Err(op("você não voltou da pausa a tempo")),
    }
}

/// `retomar`: volta a agenda suspensa por entrada recusada. Só com terminal interativo (sem
/// confirmação digitada: é voltar ao que o usuário já tinha ativado).
fn retomar(a: &mut AmbienteDoCli, resto: &[String]) -> Result<(), FalhaDoCli> {
    let [nome] = resto else {
        return Err(uso("uso: retomar <nome>"));
    };
    if !a.terminal {
        return Err(recusa_sem_terminal("retomar"));
    }
    let banco = abrir_banco(a)?;
    // Erro claro se o roteiro não existe (em vez de "não estava suspensa").
    banco.mostrar(nome)?;
    let Some(s) = banco.agenda_suspensa_escrita(nome)? else {
        w(
            a.saida,
            &format!("`{}`: a agenda não estava suspensa.", e(nome)),
        );
        return Ok(());
    };
    w(
        a.saida,
        &format!("`{}` estava suspensa: {}", e(nome), e(&s.motivo)),
    );
    banco.retomar_agenda_escrita(nome)?;
    w(a.saida, &format!("{}: agenda retomada.", e(nome)));
    Ok(())
}

fn despachar(args: &[String], a: &mut AmbienteDoCli) -> Result<(), FalhaDoCli> {
    let Some((sub, resto)) = args.split_first() else {
        return Err(uso("falta o subcomando"));
    };
    match sub.as_str() {
        "importar" => importar(a, resto),
        "listar" if resto.is_empty() => listar(a),
        "listar" => Err(uso("uso: listar")),
        "mostrar" => mostrar(a, resto),
        "testar" => testar(a, resto),
        "ativar" => ativar(a, resto),
        "desativar" => desativar(a, resto),
        "executar" => executar(a, resto),
        "retomar" => retomar(a, resto),
        "observar" => observar(a, resto),
        "entrar" => entrar(a, resto),
        outro => Err(uso(format!("subcomando desconhecido: {outro}"))),
    }
}

/// `sbf roteiros ...`: por ora só o grupo `escrito`.
pub fn rodar(args: &[String]) -> ExitCode {
    let mut args = args.to_vec();
    if matches!(
        args.first().map(String::as_str),
        Some("--ajuda" | "--help" | "-h")
    ) || args.is_empty()
    {
        println!("{AJUDA}");
        return ExitCode::SUCCESS;
    }
    if args.remove(0) != "escrito" {
        eprintln!("sbf: o grupo é `escrito`.\n{AJUDA}");
        return ExitCode::from(2);
    }
    let mut dados: Option<PathBuf> = None;
    if args.first().map(String::as_str) == Some("--dados") {
        if args.len() < 2 {
            eprintln!("sbf: --dados pede a pasta de dados.\n{AJUDA}");
            return ExitCode::from(2);
        }
        dados = Some(PathBuf::from(args[1].clone()));
        args.drain(..2);
    }
    if matches!(
        args.first().map(String::as_str),
        Some("--ajuda" | "--help" | "-h")
    ) {
        println!("{AJUDA}");
        return ExitCode::SUCCESS;
    }
    let Some(dados) = dados.or_else(|| sbf_chaveiro::pasta_de_dados_do_app().ok()) else {
        eprintln!("sbf: não achei a pasta de dados do app; passe --dados <pasta>.");
        return ExitCode::from(1);
    };
    let stdin = std::io::stdin();
    let terminal = std::io::IsTerminal::is_terminal(&stdin);
    let mut entrada = stdin.lock();
    let mut saida = std::io::stdout().lock();
    let mut ambiente = AmbienteDoCli {
        dados: &dados,
        chrome: None,
        terminal,
        entrada: &mut entrada,
        saida: &mut saida,
    };
    codigo_de_saida(despachar(&args, &mut ambiente))
}

fn codigo_de_saida(r: Result<(), FalhaDoCli>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(FalhaDoCli::Uso(m)) => {
            eprintln!("sbf: {}", e(&m));
            ExitCode::from(2)
        }
        Err(FalhaDoCli::Operacao(m)) => {
            eprintln!("sbf: {}", e(&m));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes_escrito {
    use super::*;
    use sbf_roteiros::banco::ResultadoDoTeste;
    use std::io::Cursor;

    const MANIFESTO: &str = r#"{
      "nome": "contracheque-exemplo",
      "descricao": "Contracheque fictício",
      "url_inicial": "https://portal.exemplo.test/entrar",
      "hosts": ["portal.exemplo.test"],
      "rotulo_senha": "senha:portal-exemplo",
      "rotulo_cpf": null,
      "mfa": { "tipo": "nenhum" },
      "movimenta_dinheiro": false,
      "classe": "consulta",
      "agenda_horas": 168
    }"#;
    const FONTE: &str = "navegador.abrir('https://portal.exemplo.test/entrar')\n";

    struct Cenario {
        pasta: tempfile::TempDir,
        /// O Chrome do `testar` (`None`: o que for achado).
        chrome: Option<PathBuf>,
    }

    impl Cenario {
        fn novo() -> Self {
            let pasta = tempfile::tempdir().unwrap();
            std::fs::write(pasta.path().join("manifesto.json"), MANIFESTO).unwrap();
            std::fs::write(pasta.path().join("roteiro.luau"), FONTE).unwrap();
            Self {
                pasta,
                chrome: None,
            }
        }

        fn caminho(&self, nome: &str) -> String {
            self.pasta.path().join(nome).to_string_lossy().into_owned()
        }

        fn dados(&self) -> PathBuf {
            self.pasta.path().join("dados")
        }

        /// Roda um comando; devolve o resultado e o que foi para a saída.
        fn rodar(
            &self,
            terminal: bool,
            digitado: &str,
            args: &[&str],
        ) -> (Result<(), FalhaDoCli>, String) {
            let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
            let mut entrada = Cursor::new(digitado.as_bytes().to_vec());
            let mut saida: Vec<u8> = Vec::new();
            let dados = self.dados();
            let r = despachar(
                &args,
                &mut AmbienteDoCli {
                    dados: &dados,
                    chrome: self.chrome.clone(),
                    terminal,
                    entrada: &mut entrada,
                    saida: &mut saida,
                },
            );
            (r, String::from_utf8(saida).unwrap())
        }

        fn importar(&self) -> (Result<(), FalhaDoCli>, String) {
            let m = self.caminho("manifesto.json");
            let s = self.caminho("roteiro.luau");
            self.rodar(
                false,
                "",
                &["importar", &m, "--fonte", &s, "--motivo", "primeira"],
            )
        }

        fn banco(&self) -> BancoDeRoteiros {
            BancoDeRoteiros::abrir(&self.dados().join("roteiros.sqlite")).unwrap()
        }

        fn testar_direto(&self, v: u32) {
            self.banco()
                .marcar_testado(
                    "contracheque-exemplo",
                    v,
                    &ResultadoDoTeste {
                        passou: true,
                        resumo: "ok".into(),
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn importar_cria_rascunho_e_nao_ativa_nem_sem_terminal() {
        let c = Cenario::novo();
        let (r, saida) = c.importar();
        assert_eq!(r, Ok(()));
        assert!(saida.contains("rascunho criado"), "{saida}");
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Rascunho);
        assert_eq!(v.autor, AutorEscrito::Usuario);
        assert_eq!(v.motivo, "primeira");
        assert_eq!(v.fonte, FONTE);
        assert!(
            c.banco()
                .versao_ativa_escrita("contracheque-exemplo")
                .unwrap()
                .is_none()
        );
        // o mesmo conteúdo não duplica
        let (r, saida) = c.importar();
        assert_eq!(r, Ok(()));
        assert!(saida.contains("nada foi criado"), "{saida}");
        assert_eq!(
            c.banco()
                .mostrar("contracheque-exemplo")
                .unwrap()
                .versoes
                .len(),
            1
        );
    }

    #[test]
    fn importar_recusa_manifesto_invalido_e_arquivo_ausente_sem_gravar() {
        let c = Cenario::novo();
        let ruim = MANIFESTO.replace(
            "https://portal.exemplo.test/entrar",
            "http://portal.exemplo.test/",
        );
        std::fs::write(c.pasta.path().join("ruim.json"), ruim).unwrap();
        let m = c.caminho("ruim.json");
        let s = c.caminho("roteiro.luau");
        let (r, _) = c.rodar(false, "", &["importar", &m, "--fonte", &s]);
        assert!(matches!(r, Err(FalhaDoCli::Operacao(ref e)) if e.contains("https")));
        let (r, _) = c.rodar(
            false,
            "",
            &["importar", &c.caminho("nao-existe.json"), "--fonte", &s],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
        // sem --fonte é erro de uso
        let (r, _) = c.rodar(false, "", &["importar", &c.caminho("manifesto.json")]);
        assert!(matches!(r, Err(FalhaDoCli::Uso(_))));
        assert!(c.banco().listar_escritos().unwrap().is_empty());
    }

    #[test]
    fn ativar_sem_terminal_recusa_e_nao_altera_nada() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        c.testar_direto(1);
        let (r, _) = c.rodar(
            false,
            "contracheque-exemplo@1\n",
            &["ativar", "contracheque-exemplo", "1"],
        );
        let Err(FalhaDoCli::Operacao(m)) = r else {
            panic!("devia recusar");
        };
        assert!(m.contains("só na janela ou num terminal interativo"), "{m}");
        assert!(m.contains("Nada foi alterado"), "{m}");
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Testado);
        // desativar também
        let (r, _) = c.rodar(
            false,
            "contracheque-exemplo\n",
            &["desativar", "contracheque-exemplo"],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("terminal interativo")));
    }

    #[test]
    fn ativar_com_terminal_pede_a_digitacao_exata() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        // rascunho ainda não ativa, mesmo com a digitação certa
        let (r, _) = c.rodar(
            true,
            "contracheque-exemplo@1\n",
            &["ativar", "contracheque-exemplo", "1"],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("testada")));
        c.testar_direto(1);
        // digitação errada
        let (r, saida) = c.rodar(true, "sim\n", &["ativar", "contracheque-exemplo", "1"]);
        assert!(
            matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("confirmação diferente"))
        );
        assert!(saida.contains("contracheque-exemplo@1"), "{saida}");
        assert_eq!(
            c.banco()
                .versao_escrita("contracheque-exemplo", 1)
                .unwrap()
                .estado,
            EstadoEscrito::Testado
        );
        // entrada vazia (EOF) também recusa
        let (r, _) = c.rodar(true, "", &["ativar", "contracheque-exemplo", "1"]);
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
        // digitação certa
        let (r, saida) = c.rodar(
            true,
            "contracheque-exemplo@1\n",
            &["ativar", "contracheque-exemplo", "1"],
        );
        assert_eq!(r, Ok(()));
        assert!(saida.contains("ativada"), "{saida}");
        assert_eq!(
            c.banco()
                .versao_ativa_escrita("contracheque-exemplo")
                .unwrap()
                .unwrap()
                .numero,
            1
        );
        // desativar com a digitação certa
        let (r, _) = c.rodar(
            true,
            "contracheque-exemplo\n",
            &["desativar", "contracheque-exemplo"],
        );
        assert_eq!(r, Ok(()));
        assert!(
            c.banco()
                .versao_ativa_escrita("contracheque-exemplo")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn listar_e_mostrar_descrevem_versoes_fonte_e_historico() {
        let c = Cenario::novo();
        let (r, saida) = c.rodar(false, "", &["listar"]);
        assert_eq!(r, Ok(()));
        assert!(saida.contains("Nenhum roteiro escrito"), "{saida}");
        c.importar().0.unwrap();
        let (_, saida) = c.rodar(false, "", &["listar"]);
        assert!(
            saida
                .contains("contracheque-exemplo: nenhuma versão em vigor; mais nova v1 (rascunho)"),
            "{saida}"
        );
        let (r, saida) = c.rodar(false, "", &["mostrar", "contracheque-exemplo"]);
        assert_eq!(r, Ok(()));
        assert!(saida.contains("v1: rascunho (autor usuario"), "{saida}");
        assert!(saida.contains("hosts: portal.exemplo.test"), "{saida}");
        assert!(saida.contains(FONTE.trim_end()), "{saida}");
        assert!(saida.contains("novo -> rascunho (usuario)"), "{saida}");
        let (r, _) = c.rodar(false, "", &["mostrar", "nao-existe"]);
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
        let (r, _) = c.rodar(
            false,
            "",
            &["mostrar", "contracheque-exemplo", "--versao", "7"],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
    }

    /// Importa o manifesto e o fonte do cenário como se outro autor tivesse proposto.
    fn propor_como(c: &Cenario, autor: &AutorEscrito, fonte: &str) -> u32 {
        let m = Manifesto::de_json(MANIFESTO).unwrap();
        let n = c
            .banco()
            .importar_escrito(&m, fonte, autor, "proposta")
            .unwrap()
            .numero;
        c.testar_direto(n);
        n
    }

    #[test]
    fn ativar_de_versao_do_mcp_ou_da_ia_exige_o_host_na_digitacao() {
        for autor in [AutorEscrito::Mcp, AutorEscrito::Ia("modelo-x".into())] {
            let c = Cenario::novo();
            propor_como(&c, &autor, FONTE);
            // só `nome@versão` não basta
            let (r, saida) = c.rodar(
                true,
                "contracheque-exemplo@1\n",
                &["ativar", "contracheque-exemplo", "1"],
            );
            assert!(
                matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("confirmação diferente")),
                "{r:?}"
            );
            assert!(
                saida.contains("digite exatamente `contracheque-exemplo@1 portal.exemplo.test`"),
                "{saida}"
            );
            assert_eq!(
                c.banco()
                    .versao_escrita("contracheque-exemplo", 1)
                    .unwrap()
                    .estado,
                EstadoEscrito::Testado
            );
            // com o host, ativa
            let (r, saida) = c.rodar(
                true,
                "contracheque-exemplo@1 portal.exemplo.test\n",
                &["ativar", "contracheque-exemplo", "1"],
            );
            assert_eq!(r, Ok(()), "{saida}");
            assert!(saida.contains("ativada"), "{saida}");
        }
    }

    #[test]
    fn ativar_de_versao_do_usuario_nao_pede_o_host() {
        let c = Cenario::novo();
        propor_como(&c, &AutorEscrito::Usuario, FONTE);
        let (r, saida) = c.rodar(
            true,
            "contracheque-exemplo@1\n",
            &["ativar", "contracheque-exemplo", "1"],
        );
        assert_eq!(r, Ok(()), "{saida}");
        assert!(
            saida.contains("digite exatamente `contracheque-exemplo@1`"),
            "{saida}"
        );
        assert!(!saida.contains("PROPOSTO POR"), "{saida}");
    }

    #[test]
    fn ativar_mostra_em_destaque_autor_hosts_rotulo_mfa_e_fonte() {
        let c = Cenario::novo();
        propor_como(&c, &AutorEscrito::Mcp, FONTE);
        let (_, saida) = c.rodar(true, "", &["ativar", "contracheque-exemplo", "1"]);
        for trecho in [
            "CONFIRA ANTES DE ATIVAR",
            "AUTOR DA VERSÃO: mcp",
            "HOSTS",
            "portal.exemplo.test",
            "RÓTULO DA SENHA no chaveiro: senha:portal-exemplo",
            "MFA POR E-MAIL: não",
            "PROPOSTO POR mcp",
            "movimenta_dinheiro: false (declarado pelo autor",
            "Fonte Luau (1 linhas)",
            "navegador.abrir('https://portal.exemplo.test/entrar')",
        ] {
            assert!(saida.contains(trecho), "falta `{trecho}` em:\n{saida}");
        }
        // tudo isso vem ANTES do pedido de digitação
        let pedido = saida.find("Para confirmar").unwrap();
        assert!(saida.find("CONFIRA ANTES DE ATIVAR").unwrap() < pedido);
        assert!(saida.find("Fonte Luau").unwrap() < pedido);
    }

    #[test]
    fn ativar_com_mfa_por_email_avisa_que_le_o_gmail() {
        let c = Cenario::novo();
        let com_email = MANIFESTO.replace(
            r#""mfa": { "tipo": "nenhum" }"#,
            r#""mfa": { "tipo": "email", "remetente": "nao-responda@portal.exemplo.test", "formato": "\\d{6}" }"#,
        );
        let m = Manifesto::de_json(&com_email).unwrap();
        c.banco()
            .importar_escrito(&m, FONTE, &AutorEscrito::Usuario, "")
            .unwrap();
        c.testar_direto(1);
        let (_, saida) = c.rodar(true, "", &["ativar", "contracheque-exemplo", "1"]);
        assert!(saida.contains("MFA POR E-MAIL: SIM"), "{saida}");
        assert!(saida.contains("Gmail"), "{saida}");
    }

    #[test]
    fn ativar_com_fonte_grande_mostra_so_linhas_e_checksum() {
        let c = Cenario::novo();
        let grande = "navegador.registrar('x')\n".repeat(250);
        propor_como(&c, &AutorEscrito::Usuario, &grande);
        let (_, saida) = c.rodar(true, "", &["ativar", "contracheque-exemplo", "1"]);
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert!(saida.contains("250 linhas"), "{saida}");
        assert!(saida.contains(&v.checksum), "{saida}");
        assert!(
            saida.contains("sbf roteiros escrito mostrar contracheque-exemplo --versao 1"),
            "{saida}"
        );
        assert!(!saida.contains("navegador.registrar('x')"), "{saida}");
    }

    /// Um rascunho hostil gravado direto no banco (a validação do importar não o deixaria
    /// entrar): controle e direção de texto em todo campo que o CLI imprime.
    fn hostil(c: &Cenario) {
        let banco = c.banco();
        drop(banco);
        let conn = rusqlite::Connection::open(c.dados().join("roteiros.sqlite")).unwrap();
        conn.execute(
            "INSERT INTO roteiro_escrito (nome) VALUES ('contracheque-exemplo')",
            [],
        )
        .unwrap();
        let manifesto = MANIFESTO
            .replace("Contracheque fictício", "desc\\u001b[3Apresa\\r\\u202efdp")
            .replace("senha:portal-exemplo", "rotulo\\u001b[2Kx\\u202e");
        let manifesto: serde_json::Value = serde_json::from_str(&manifesto).unwrap();
        conn.execute(
            "INSERT INTO roteiro_escrito_versao
               (roteiro_id, numero, manifesto, fonte, checksum, estado, autor, motivo)
             VALUES (1, 1, ?1, ?2, 'abc', 'testado', 'ia:evil\u{1b}[31m', ?3)",
            rusqlite::params![
                manifesto.to_string(),
                "navegador.registrar('a')\u{1b}[3A\rfalso\n",
                "motivo\u{1b}[3A\r\u{202e}\n[OK] linha forjada"
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             VALUES (1, NULL, 'rascunho', 'evil\u{1b}[0m', 'det\u{202e}alhe\r')",
            [],
        )
        .unwrap();
    }

    fn sem_perigo(saida: &str) {
        for c in ['\u{1b}', '\r', '\u{202e}', '\0'] {
            assert!(!saida.contains(c), "{c:?} chegou ao terminal:\n{saida:?}");
        }
    }

    #[test]
    fn mostrar_listar_e_ativar_escapam_controle_e_direcao_do_banco() {
        let c = Cenario::novo();
        hostil(&c);
        let (r, saida) = c.rodar(false, "", &["mostrar", "contracheque-exemplo"]);
        assert_eq!(r, Ok(()), "{saida}");
        sem_perigo(&saida);
        for visivel in [
            "\\x1b[3A",
            "\\x0d",
            "\\u{202e}",
            "autor ia:evil\\x1b[31m",
            "motivo\\x1b[3A\\x0d\\u{202e}\\x0a[OK] linha forjada",
            "evil\\x1b[0m",
        ] {
            assert!(saida.contains(visivel), "falta `{visivel}` em:\n{saida}");
        }
        // o `\n` do motivo não forja uma linha nova "[OK]"
        assert!(
            !saida.lines().any(|l| l.starts_with("[OK]")),
            "linha forjada:\n{saida}"
        );
        let (_, saida) = c.rodar(false, "", &["listar"]);
        sem_perigo(&saida);
        let (r, saida) = c.rodar(true, "", &["ativar", "contracheque-exemplo", "1"]);
        assert!(r.is_err());
        sem_perigo(&saida);
        assert!(saida.contains("\\x1b[3A"), "{saida}");
        assert!(saida.contains("rotulo\\x1b[2Kx\\u{202e}"), "{saida}");
    }

    #[test]
    fn importar_recusa_fonte_com_escape_e_aceita_crlf() {
        let c = Cenario::novo();
        let m = c.caminho("manifesto.json");
        std::fs::write(
            c.pasta.path().join("sujo.luau"),
            "navegador.registrar('a')\u{1b}[3A\n",
        )
        .unwrap();
        let (r, _) = c.rodar(
            false,
            "",
            &["importar", &m, "--fonte", &c.caminho("sujo.luau")],
        );
        assert!(
            matches!(r, Err(FalhaDoCli::Operacao(ref e)) if e.contains("controle")),
            "{r:?}"
        );
        std::fs::write(
            c.pasta.path().join("windows.luau"),
            "navegador.registrar('a')\r\nnavegador.registrar('b')\r\n",
        )
        .unwrap();
        let (r, _) = c.rodar(
            false,
            "",
            &["importar", &m, "--fonte", &c.caminho("windows.luau")],
        );
        assert_eq!(r, Ok(()));
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(
            v.fonte,
            "navegador.registrar('a')\nnavegador.registrar('b')\n"
        );
        // a opção antiga não existe mais
        let (r, _) = c.rodar(
            false,
            "",
            &["importar", &m, "--script", &c.caminho("roteiro.luau")],
        );
        assert!(matches!(r, Err(FalhaDoCli::Uso(_))), "{r:?}");
    }

    fn codigo(r: Result<(), FalhaDoCli>) -> String {
        format!("{:?}", codigo_de_saida(r))
    }

    #[test]
    fn testar_sem_cenario_lista_os_cenarios_e_sai_com_codigo_2() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        let (r, saida) = c.rodar(false, "", &["testar", "contracheque-exemplo", "1"]);
        let Err(FalhaDoCli::Uso(m)) = &r else {
            panic!("devia ser erro de uso");
        };
        assert!(m.contains("--cenario"), "{m}");
        assert!(saida.contains("Cenários disponíveis"), "{saida}");
        assert!(saida.contains("contracheque"), "{saida}");
        assert_eq!(codigo(r), format!("{:?}", ExitCode::from(2)));
        // nada rodou: continua rascunho, sem resultado de teste
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Rascunho);
        assert!(v.resultado_do_teste.is_none());
    }

    #[test]
    fn testar_com_cenario_desconhecido_ou_uso_errado_sai_com_codigo_2() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        for args in [
            vec!["testar", "contracheque-exemplo", "1", "--cenario", "outro"],
            vec![
                "testar",
                "contracheque-exemplo",
                "--cenario",
                "contracheque",
            ],
            vec![
                "testar",
                "contracheque-exemplo",
                "zero",
                "--cenario",
                "contracheque",
            ],
            vec!["testar", "contracheque-exemplo", "1", "--cenario"],
        ] {
            let (r, _) = c.rodar(false, "", &args);
            assert!(matches!(r, Err(FalhaDoCli::Uso(_))), "{args:?}: {r:?}");
        }
    }

    #[test]
    fn testar_roteiro_de_outro_cenario_e_erro_claro_com_codigo_1() {
        // o manifesto deste arquivo não tem MFA por e-mail: não é o contracheque do site falso
        let c = Cenario::novo();
        c.importar().0.unwrap();
        let (r, _) = c.rodar(
            false,
            "",
            &[
                "testar",
                "contracheque-exemplo",
                "1",
                "--cenario",
                "contracheque",
            ],
        );
        let Err(FalhaDoCli::Operacao(m)) = &r else {
            panic!("devia ser erro de operação: {r:?}");
        };
        assert!(
            m.contains("este roteiro não é do cenário contracheque"),
            "{m}"
        );
        assert_eq!(codigo(r), format!("{:?}", ExitCode::from(1)));
        assert_eq!(
            c.banco()
                .versao_escrita("contracheque-exemplo", 1)
                .unwrap()
                .estado,
            EstadoEscrito::Rascunho
        );
    }

    #[test]
    fn testar_versao_ou_roteiro_inexistente_e_erro_de_operacao() {
        let c = Cenario::novo();
        let (r, _) = c.rodar(
            false,
            "",
            &["testar", "nao-existe", "1", "--cenario", "contracheque"],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))), "{r:?}");
    }

    /// Importa os exemplos de `crates/sbf-roteiros/exemplos` (o contracheque fictício).
    fn importar_exemplos(c: &Cenario) {
        let exemplos = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sbf-roteiros/exemplos");
        let (r, _) = c.rodar(
            false,
            "",
            &[
                "importar",
                &exemplos.join("contracheque-exemplo.json").to_string_lossy(),
                "--fonte",
                &exemplos.join("contracheque-exemplo.luau").to_string_lossy(),
            ],
        );
        r.unwrap();
    }

    #[test]
    fn testar_sem_chrome_e_erro_claro_e_nao_muda_nada() {
        let mut c = Cenario::novo();
        c.chrome = Some(c.pasta.path().join("nao-existe").join("chrome"));
        importar_exemplos(&c);
        let (r, _) = c.rodar(
            false,
            "",
            &[
                "testar",
                "contracheque-exemplo",
                "1",
                "--cenario",
                "contracheque",
            ],
        );
        let Err(FalhaDoCli::Operacao(m)) = &r else {
            panic!("devia ser erro de operação: {r:?}");
        };
        assert!(m.contains("Chrome"), "{m}");
        assert_eq!(codigo(r), format!("{:?}", ExitCode::from(1)));
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Rascunho);
    }

    #[test]
    fn testar_o_exemplo_com_chrome_passa_sem_terminal_e_vira_testado() {
        if std::env::var("SBF_CHROME_TESTE").ok().as_deref() != Some("1") {
            eprintln!("pulado: defina SBF_CHROME_TESTE=1 para rodar com o Chrome");
            return;
        }
        let c = Cenario::novo();
        importar_exemplos(&c);
        // `terminal = false`: testar não exige terminal interativo
        let (r, saida) = c.rodar(
            false,
            "",
            &[
                "testar",
                "contracheque-exemplo",
                "1",
                "--cenario",
                "contracheque",
            ],
        );
        r.unwrap();
        assert!(saida.contains("PASSOU"), "{saida}");
        assert!(!saida.contains("SenhaFicticia"), "{saida}");
        let v = c.banco().versao_escrita("contracheque-exemplo", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Testado);
        // e o cofre nem foi criado
        assert!(!c.dados().join("cofre-do-teste-nao-usado").exists());
        assert_eq!(codigo(Ok(())), format!("{:?}", ExitCode::SUCCESS));
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn executar_monta_o_pedido_com_meses_e_forcar() {
        let p = montar_execucao(
            &args(&[
                "contracheque-exemplo",
                "--inventario",
                "/cofre",
                "--de",
                "-1",
                "--ate",
                "2026-11",
                "--forcar",
            ]),
            true,
        )
        .unwrap();
        assert_eq!(p.nome, "contracheque-exemplo");
        assert_eq!(p.cofre, PathBuf::from("/cofre"));
        assert!(p.pedido.forcar);
        assert_eq!(
            p.pedido.entregues.get("de"),
            Some(&ValorBruto::Texto("-1".into()))
        );
        assert_eq!(
            p.pedido.entregues.get("ate"),
            Some(&ValorBruto::Texto("2026-11".into()))
        );
        assert_eq!((p.pedido.entrada, p.pedido.previsto_unix), (None, None));
        // sem opções: pedido vazio (valem os padrões do manifesto)
        let p = montar_execucao(&args(&["x", "--inventario", "/c"]), false).unwrap();
        assert_eq!(p.pedido, PedidoDeExecucao::default());
        // a ordem das opções não importa
        let p = montar_execucao(
            &args(&["--forcar", "--inventario", "/c", "x", "--ate", "0"]),
            true,
        )
        .unwrap();
        assert!(p.pedido.forcar);
        assert_eq!(p.nome, "x");
    }

    #[test]
    fn executar_recusa_uso_errado_e_forcar_sem_terminal() {
        for ruim in [
            vec![],
            vec!["x"],                                               // sem --inventario
            vec!["--inventario", "/c"],                              // sem nome
            vec!["a", "b", "--inventario", "/c"],                    // dois nomes
            vec!["x", "--inventario", "/c", "--de"],                 // sem valor
            vec!["x", "--inventario", "/c", "--de", "ontem"],        // mês ilegível
            vec!["x", "--inventario", "/c", "--ate", "2026-13"],     // mês fora do calendário
            vec!["x", "--inventario", "/c", "--de", "1"],            // relativo tem de ser <= 0
            vec!["x", "--inventario", "/c", "--forcar", "--forcar"], // repetido
            vec!["x", "--inventario", "/c", "--de", "0", "--de", "0"],
            vec!["x", "--inventario", "/c", "--senha", "abc"], // opção desconhecida
        ] {
            let r = montar_execucao(&args(&ruim), true);
            assert!(matches!(r, Err(FalhaDoCli::Uso(_))), "{ruim:?}: {r:?}");
        }
        // forçar sem terminal é recusa de operação (o MCP e a IA só propõem)
        let r = montar_execucao(&args(&["x", "--inventario", "/c", "--forcar"]), false);
        assert!(
            matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("terminal")),
            "{r:?}"
        );
    }

    #[test]
    fn executar_pelo_despachante_recusa_uso_errado_sem_abrir_nada() {
        let c = Cenario::novo();
        let (r, saida) = c.rodar(true, "", &["executar", "x"]);
        assert!(matches!(r, Err(FalhaDoCli::Uso(_))));
        assert!(saida.is_empty(), "{saida}");
        // nem o banco foi criado
        assert!(!c.dados().join("roteiros.sqlite").exists());
        let (r, _) = c.rodar(
            false,
            "",
            &["executar", "x", "--inventario", "/c", "--forcar"],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
        assert!(!c.dados().join("roteiros.sqlite").exists());
    }

    #[test]
    fn retomar_sem_terminal_recusa_e_nao_altera() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        c.banco()
            .suspender_agenda_escrita("contracheque-exemplo", "o portal pediu a senha nova", 1)
            .unwrap();
        let (r, _) = c.rodar(false, "", &["retomar", "contracheque-exemplo"]);
        assert!(
            matches!(r, Err(FalhaDoCli::Operacao(ref m)) if m.contains("terminal")),
            "{r:?}"
        );
        assert!(
            c.banco()
                .agenda_suspensa_escrita("contracheque-exemplo")
                .unwrap()
                .is_some()
        );
        // uso errado
        for ruim in [vec!["retomar"], vec!["retomar", "a", "b"]] {
            let (r, _) = c.rodar(true, "", &ruim);
            assert!(matches!(r, Err(FalhaDoCli::Uso(_))), "{ruim:?}");
        }
    }

    #[test]
    fn retomar_com_terminal_volta_a_agenda_e_listar_mostrar_avisam_antes() {
        let c = Cenario::novo();
        c.importar().0.unwrap();
        c.banco()
            .suspender_agenda_escrita("contracheque-exemplo", "o portal pediu a senha nova", 1)
            .unwrap();
        let (_, saida) = c.rodar(false, "", &["listar"]);
        assert!(saida.contains("SUSPENSA"), "{saida}");
        let (_, saida) = c.rodar(false, "", &["mostrar", "contracheque-exemplo"]);
        assert!(saida.contains("AGENDA SUSPENSA"), "{saida}");
        assert!(saida.contains("o portal pediu a senha nova"), "{saida}");
        assert!(saida.contains("retomar contracheque-exemplo"), "{saida}");

        let (r, saida) = c.rodar(true, "", &["retomar", "contracheque-exemplo"]);
        assert_eq!(r, Ok(()));
        assert!(saida.contains("agenda retomada"), "{saida}");
        assert!(
            c.banco()
                .agenda_suspensa_escrita("contracheque-exemplo")
                .unwrap()
                .is_none()
        );
        // de novo: nada a retomar, sem erro
        let (r, saida) = c.rodar(true, "", &["retomar", "contracheque-exemplo"]);
        assert_eq!(r, Ok(()));
        assert!(saida.contains("não estava suspensa"), "{saida}");
        // roteiro inexistente: erro de operação
        let (r, _) = c.rodar(true, "", &["retomar", "nao-existe"]);
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))), "{r:?}");
    }

    #[test]
    fn mostrar_descreve_a_agenda_e_os_parametros_do_manifesto() {
        let c = Cenario::novo();
        let com_agenda = MANIFESTO.replace(
            r#""agenda_horas": 168"#,
            r#""agenda": [{"horarios": ["04:00", "20:00"], "dias_do_mes": [20, 30], "parametros": {"de": "-1"}}],
               "parametros": [{"nome": "de", "tipo": "mes", "padrao": "0"}]"#,
        );
        std::fs::write(c.pasta.path().join("manifesto.json"), com_agenda).unwrap();
        c.importar().0.unwrap();
        let (_, saida) = c.rodar(false, "", &["mostrar", "contracheque-exemplo"]);
        assert!(saida.contains("agenda: por horário"), "{saida}");
        assert!(
            saida.contains("entrada 1: 04:00, 20:00 nos dias 20, 30 (de=-1)"),
            "{saida}"
        );
        assert!(saida.contains("parâmetro de: mês, padrão 0"), "{saida}");
    }

    #[test]
    fn subcomando_desconhecido_e_versao_invalida_sao_erro_de_uso() {
        let c = Cenario::novo();
        for args in [
            vec!["voar"],
            vec![],
            vec!["ativar", "a"],
            vec!["ativar", "a", "0"],
            vec!["ativar", "a", "x"],
            vec!["listar", "extra"],
            vec!["mostrar"],
            vec!["importar", "--desconhecida"],
        ] {
            let (r, _) = c.rodar(true, "", &args);
            assert!(matches!(r, Err(FalhaDoCli::Uso(_))), "{args:?}");
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn o_roteiro_lento_nao_trava_o_laco_e_o_resultado_chega_na_passada_seguinte() {
        let nao = AtomicBool::new(false);
        let mut f = ExecucaoEmFio::nova();
        let t0 = Instant::now();
        let r = f.passar(&nao, |_| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(ResumoDeRoteiros {
                executados: 1,
                baixados: 2,
                novos: 1,
            })
        });
        assert!(
            t0.elapsed() < Duration::from_millis(200),
            "a passada não espera"
        );
        assert_eq!(r.unwrap(), ResumoDeRoteiros::default());
        assert!(f.rodando());
        // enquanto roda, outra passada não inicia nada nem bloqueia
        let t1 = Instant::now();
        let r = f.passar(&nao, |_| panic!("não podia iniciar de novo"));
        assert!(t1.elapsed() < Duration::from_millis(200));
        assert_eq!(r.unwrap(), ResumoDeRoteiros::default());
        std::thread::sleep(Duration::from_millis(600));
        let r = f.passar(&nao, |_| Ok(ResumoDeRoteiros::default())).unwrap();
        assert_eq!((r.executados, r.baixados, r.novos), (1, 2, 1));
    }

    #[test]
    fn cancelar_o_worker_cancela_o_roteiro_em_curso() {
        let nao = AtomicBool::new(false);
        let mut f = ExecucaoEmFio::nova();
        let _ = f.passar(&nao, |c| {
            while !c.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(ResumoDeRoteiros::default())
        });
        let sim = AtomicBool::new(true);
        let _ = f.passar(&sim, |_| Ok(ResumoDeRoteiros::default()));
        let limite = Instant::now() + Duration::from_secs(2);
        while f.rodando() && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!f.rodando());
    }
}
