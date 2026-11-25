//! Os roteiros de verdade no `sbf worker` (spec 10, C1): executa os agendados com o Chrome e o
//! chaveiro do usuário e, se um passo falha, pede à IA (o `claude` do usuário, via o mesmo motor
//! da triagem) um conserto que **fica como proposta** até o usuário aprovar na janela.
//!
//! Também é o CLI do **roteiro escrito** (Luau), `sbf roteiros escrito`:
//!
//! ```text
//! sbf roteiros escrito [--dados <pasta-de-dados>] <subcomando>
//!   importar <manifesto.json> --script <arquivo.luau> [--motivo <texto>]   (cria rascunho)
//!   listar | mostrar <nome> [--versao <n>] | testar <nome> <versao>
//!   ativar <nome> <versao> | desativar <nome>   (só em terminal interativo, com confirmação)
//! ```
//!
//! O catálogo é o `roteiros.sqlite` da pasta de dados do app, fora do cofre. Ver [`AJUDA`].
//! `ativar` e `desativar` só rodam com stdin num terminal e depois de a pessoa digitar
//! exatamente `<nome>@<versao>` (ou `<nome>`): o MCP e a IA, que não têm terminal, só
//! importam rascunhos. Nenhuma opção nem variável de ambiente pula isso.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_ia::{NivelDeConfianca, Registro};
use sbf_roteiros::banco::{AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoEscrito};
use sbf_roteiros::executor::FonteDeSenhas;
use sbf_roteiros::gravacao::Guia;
use sbf_roteiros::manifesto::{Classe, Manifesto, Mfa};
use sbf_roteiros::servico::{Ambiente, executar_devidos};
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
        if banco.devidos(agora_i).map_err(|_| Falha::Banco)?.is_empty() {
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
    Ok(ResumoDeRoteiros {
        executados: r.executados,
        baixados: r.baixados as usize,
        novos: r.novos as usize,
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

  importar <manifesto.json> --script <arquivo.luau> [--motivo <texto>]
                                         cria um RASCUNHO (nunca ativa); o mesmo conteúdo
                                         da versão mais nova não duplica
  listar                                 os roteiros escritos, a versão em vigor e a mais nova
  mostrar <nome> [--versao <n>]          manifesto, fonte, versões e histórico de estados
  testar <nome> <versao>                 roda no site falso (ainda não liga: sai com código 2)
  ativar <nome> <versao>                 só num terminal interativo, com confirmação digitada;
                                         só de uma versão testada; aposenta a ativa anterior
  desativar <nome>                       idem (aposenta a versão ativa)

O roteiro escrito vive em `roteiros.sqlite`, na pasta de dados do app (fora do cofre).
Ativar e desativar não rodam sem terminal: o MCP e a IA só propõem (criam rascunho).";

/// O que dá errado, já em português para o terminal.
#[derive(Debug, PartialEq, Eq)]
enum FalhaDoCli {
    /// Uso errado da linha de comando (código 2).
    Uso(String),
    /// FalhaDoCli da operação (código 1).
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
    terminal: bool,
    entrada: &'a mut dyn std::io::BufRead,
    saida: &'a mut dyn std::io::Write,
}

fn w(saida: &mut dyn std::io::Write, linha: &str) {
    let _ = writeln!(saida, "{linha}");
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

fn mfa_texto(m: &Mfa) -> String {
    match m {
        Mfa::Nenhum => "nenhum".to_owned(),
        Mfa::Manual => "manual (o roteiro pausa e a vez é sua)".to_owned(),
        Mfa::Email { remetente, formato } => {
            format!("código por e-mail de {remetente}, formato {formato}")
        }
    }
}

fn resumo_do_manifesto(m: &Manifesto) -> Vec<String> {
    vec![
        format!("  hosts: {}", m.hosts.join(", ")),
        format!("  url inicial: {}", m.url_inicial),
        format!("  mfa: {}", mfa_texto(&m.mfa)),
        format!(
            "  movimenta dinheiro: {}",
            if m.movimenta_dinheiro { "sim" } else { "não" }
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
            m.agenda_horas
                .map_or_else(|| "sem agenda".to_owned(), |h| format!("a cada {h} h"))
        ),
        format!(
            "  chaveiro: senha {}, CPF {}",
            m.rotulo_senha.as_deref().unwrap_or("não usa"),
            m.rotulo_cpf.as_deref().unwrap_or("não usa")
        ),
    ]
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
    let (soltos, valores) = opcoes(resto, &["--script", "--motivo"])?;
    let [arquivo] = soltos.as_slice() else {
        return Err(uso(
            "uso: importar <manifesto.json> --script <arquivo.luau> [--motivo <texto>]",
        ));
    };
    let script = valores
        .get("--script")
        .ok_or_else(|| uso("importar pede --script <arquivo.luau>"))?;
    let manifesto = Manifesto::de_json(&ler_texto(Path::new(arquivo))?)?;
    let fonte = ler_texto(Path::new(script))?;
    let motivo = valores.get("--motivo").map_or("", String::as_str);
    let banco = abrir_banco(a)?;
    let i = banco.importar_escrito(&manifesto, &fonte, &AutorEscrito::Usuario, motivo)?;
    if i.criada {
        w(
            a.saida,
            &format!(
                "{} v{}: rascunho criado. Falta testar no site falso e ativar num terminal interativo.",
                manifesto.nome, i.numero
            ),
        );
    } else {
        w(
            a.saida,
            &format!(
                "{} v{}: já é a versão mais nova com este conteúdo; nada foi criado.",
                manifesto.nome, i.numero
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
        w(
            a.saida,
            &format!(
                "{}: {em_vigor}; mais nova v{} ({})",
                l.nome,
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
    w(a.saida, &format!("{}:", d.nome));
    for v in &d.versoes {
        w(
            a.saida,
            &format!(
                "  v{}: {} (autor {}, checksum {}, {}){}{}",
                v.numero,
                v.estado.texto(),
                v.autor.texto(),
                curto(&v.checksum),
                v.criada_em,
                if v.motivo.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", v.motivo)
                },
                v.resultado_do_teste
                    .as_ref()
                    .map_or(String::new(), |t| format!(" [teste {t}]")),
            ),
        );
    }
    let v = d
        .versoes
        .iter()
        .find(|v| v.numero == escolhida)
        .ok_or_else(|| op(format!("`{nome}` não tem a versão {escolhida}")))?;
    w(a.saida, &format!("Manifesto da v{}:", v.numero));
    if !v.manifesto.descricao.is_empty() {
        w(a.saida, &format!("  {}", v.manifesto.descricao));
    }
    for l in resumo_do_manifesto(&v.manifesto) {
        w(a.saida, &l);
    }
    w(a.saida, &format!("Fonte da v{}:", v.numero));
    w(a.saida, &v.fonte);
    w(a.saida, "Histórico:");
    for (quando, versao, de, para, quem, detalhe) in &d.eventos {
        w(
            a.saida,
            &format!(
                "  {quando} v{versao}: {} -> {para} ({quem}){}",
                de.as_deref().unwrap_or("novo"),
                if detalhe.is_empty() {
                    String::new()
                } else {
                    format!(" {detalhe}")
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
    let _ = write!(a.saida, "Para confirmar, digite exatamente `{esperado}`: ");
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
            "só uma versão testada pode ser ativada; `{nome}` v{versao} está {}",
            v.estado.texto()
        )));
    }
    let mut resumo = vec![format!(
        "ativar `{nome}` v{versao} (autor {}, checksum {}): {}",
        v.autor.texto(),
        curto(&v.checksum),
        v.motivo
    )];
    resumo.extend(resumo_do_manifesto(&v.manifesto));
    let esperado = Confirmacao::texto_de_ativar(nome, versao);
    let conf = confirmar(a, &resumo, &esperado)?;
    banco.ativar(nome, versao, &conf)?;
    w(a.saida, &format!("{nome} v{versao}: ativada."));
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
        return Err(op(format!("`{nome}` não tem versão ativa")));
    };
    let resumo = vec![format!("desativar `{nome}` (v{} em vigor)", v.numero)];
    let conf = confirmar(a, &resumo, &Confirmacao::texto_de_desativar(nome))?;
    let n = banco.desativar(nome, &conf)?;
    w(a.saida, &format!("{nome} v{n}: desativada."));
    Ok(())
}

fn testar(resto: &[String]) -> Result<(), FalhaDoCli> {
    let [_nome, versao] = resto else {
        return Err(uso("uso: testar <nome> <versao>"));
    };
    versao_de(versao)?;
    Err(uso("o teste no site falso chega na próxima etapa"))
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
        "testar" => testar(resto),
        "ativar" => ativar(a, resto),
        "desativar" => desativar(a, resto),
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
            eprintln!("sbf: {m}");
            ExitCode::from(2)
        }
        Err(FalhaDoCli::Operacao(m)) => {
            eprintln!("sbf: {m}");
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
    }

    impl Cenario {
        fn novo() -> Self {
            let pasta = tempfile::tempdir().unwrap();
            std::fs::write(pasta.path().join("manifesto.json"), MANIFESTO).unwrap();
            std::fs::write(pasta.path().join("roteiro.luau"), FONTE).unwrap();
            Self { pasta }
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
                &["importar", &m, "--script", &s, "--motivo", "primeira"],
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
        let (r, _) = c.rodar(false, "", &["importar", &m, "--script", &s]);
        assert!(matches!(r, Err(FalhaDoCli::Operacao(ref e)) if e.contains("https")));
        let (r, _) = c.rodar(
            false,
            "",
            &["importar", &c.caminho("nao-existe.json"), "--script", &s],
        );
        assert!(matches!(r, Err(FalhaDoCli::Operacao(_))));
        // sem --script é erro de uso
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

    #[test]
    fn testar_ainda_nao_liga_e_sai_com_codigo_2() {
        let c = Cenario::novo();
        let (r, _) = c.rodar(false, "", &["testar", "contracheque-exemplo", "1"]);
        let Err(FalhaDoCli::Uso(m)) = &r else {
            panic!("devia ser erro de uso");
        };
        assert!(
            m.contains("o teste no site falso chega na próxima etapa"),
            "{m}"
        );
        assert_eq!(
            format!("{:?}", codigo_de_saida(r)),
            format!("{:?}", ExitCode::from(2))
        );
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
