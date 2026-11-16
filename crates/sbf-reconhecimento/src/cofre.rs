//! O reconhecedor real: [`ReconhecedorDoCofre`] roda as seis etapas sobre o cofre.
//!
//! Escreve só no cofre: o achado, a origem e a cópia no acervo (sbf-coleta), o texto
//! (sbf-inventario) e a sugestão da IA (sbf-triagem). **Nunca** muda nem apaga o arquivo
//! solto, e nunca escreve no planejamento: as [`Sugestao`]es só viram alteração no clique do
//! usuário.
//!
//! Nenhum conteúdo de documento vai para log nem para mensagem de erro: só contagens e
//! motivos genéricos.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OpenFlags, OptionalExtension as _};
use sbf_coleta::pastas::{registrar_arrastado, sha256_do_arquivo};
use sbf_ia::{Controle, Fila, Provedor, Registro};
use sbf_inventario::sugestao::RegistroDaIa;
use sbf_inventario::texto::{self, METODO_TEXTO_SIMPLES};
use sbf_pdf::texto::{LIMITE_DO_TEXTO, cortar_como_o_js, impressao_canonica};
use sbf_pdf::{Desfecho, Ferramentas, FonteDeSegredos, OpcoesDaRodada, PdfProtegido};
use sbf_planejamento::{Banco, Planejamento as _, PlanejamentoSqlite, SemSaldos, TipoDeDocumento};
use sbf_triagem::Alvo;
use sbf_triagem::rodada::{
    ConfigDaRodada, Progresso, RastroDeFalhas, SituacaoDaRodada, configuracao_da_fila, rodar,
};

use crate::salario::{
    ErroDoSalario, composicao_ja_vigente, ja_vinculado, recorrente_do_salario, valor_digitado,
};
use crate::{
    Andamento, ErroDoReconhecimento, EstadoDoTexto, Etapa, Reconhecedor, Reconhecido, Resultado,
    Situacao, Sugestao, UsoDeRubricas, ocorrencia_do_mes,
};

/// A fonte de segredos do reconhecedor (o chaveiro, na janela).
pub type Segredos = Arc<dyn FonteDeSegredos + Send + Sync>;

/// O maior arquivo `.txt` lido para o inventário.
const LIMITE_DO_TXT: u64 = 2 * 1024 * 1024;

/// O motor de IA da triagem: a mesma fila (provedor principal, reserva, nível de confiança) e
/// o mesmo registro do que saiu (`saida_ia`) que a área Triagem usa. A janela monta com
/// `sbf::triagem::montar_motor` e entrega aqui.
#[derive(Clone)]
pub struct MotorDeIa {
    /// A fila do motor (cache, ritmo, degraus).
    pub fila: Arc<Fila>,
    /// O registro de `saida_ia`, marcado com o id de cada rodada.
    pub registro: Arc<RegistroDaIa>,
    /// O nome do provedor principal (vai em `rodada_ia.provedor`).
    pub provedor: &'static str,
    /// O rastro de falhas transitórias (o item falho é adiado, não "não classificado").
    pub rastro: Option<RastroDeFalhas>,
}

impl std::fmt::Debug for MotorDeIa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorDeIa")
            .field("provedor", &self.provedor)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
enum Ia {
    Nenhuma,
    /// Só o provedor: a fila nasce na hora, com o prompt e o nível da triagem.
    Provedor(Arc<dyn Provedor>),
    Motor(MotorDeIa),
}

/// O reconhecedor sobre uma pasta de cofre.
pub struct ReconhecedorDoCofre {
    cofre: PathBuf,
    segredos: Segredos,
    ia: Ia,
    ferramentas: Ferramentas,
}

impl std::fmt::Debug for ReconhecedorDoCofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReconhecedorDoCofre")
            .field("cofre", &self.cofre)
            .field("ia", &!matches!(self.ia, Ia::Nenhuma))
            .finish_non_exhaustive()
    }
}

impl ReconhecedorDoCofre {
    /// `cofre` é a pasta do cofre (com `inventario.sqlite` já migrado e, se houver,
    /// `planejamento.sqlite`). `segredos` abre PDF protegido pelas regras do chaveiro.
    /// `provedor` é o da triagem; `None` deixa a etapa da IA pulada ("IA não configurada").
    ///
    /// Nada é aberto aqui: cada [`Reconhecedor::reconhecer`] abre as próprias conexões.
    pub fn novo(
        cofre: impl Into<PathBuf>,
        segredos: Segredos,
        provedor: Option<Arc<dyn Provedor>>,
    ) -> Self {
        Self {
            cofre: cofre.into(),
            segredos,
            ia: provedor.map_or(Ia::Nenhuma, Ia::Provedor),
            ferramentas: Ferramentas::default(),
        }
    }

    /// Usa o motor da triagem (a mesma fila, reserva e registro da área Triagem) em vez de
    /// montar uma fila só com o provedor.
    #[must_use]
    pub fn com_motor_de_ia(mut self, motor: MotorDeIa) -> Self {
        self.ia = Ia::Motor(motor);
        self
    }

    /// As ferramentas externas (`pdftotext`, `qpdf`) de Opções.
    #[must_use]
    pub fn com_ferramentas(mut self, ferramentas: Ferramentas) -> Self {
        self.ferramentas = ferramentas;
        self
    }
}

fn banco(e: impl std::fmt::Display) -> ErroDoReconhecimento {
    ErroDoReconhecimento::Banco(e.to_string())
}

fn do_arquivo(e: &std::io::Error) -> ErroDoReconhecimento {
    ErroDoReconhecimento::Arquivo(match e.kind() {
        std::io::ErrorKind::NotFound => "o arquivo não existe mais".to_owned(),
        std::io::ErrorKind::PermissionDenied => "sem permissão para ler o arquivo".to_owned(),
        _ => "falha ao ler o arquivo".to_owned(),
    })
}

fn da_coleta(e: sbf_coleta::Erro) -> ErroDoReconhecimento {
    match e {
        sbf_coleta::Erro::Io(e) => do_arquivo(&e),
        outro => banco(outro),
    }
}

/// O que o arquivo já disse até agora, entre as etapas.
struct Estado {
    sha256: String,
    ja_existia: bool,
    dentro_do_cofre: bool,
    /// O achado não está no inventário (arquivo de dentro do cofre que ele não conhece).
    sem_achado: bool,
    texto: EstadoDoTexto,
    /// O texto para os leitores e a IA; `None` quando não há.
    conteudo: Option<String>,
    reconhecido: Reconhecido,
    sugestoes: Vec<Sugestao>,
    avisos: Vec<String>,
}

type Saida<T> = Result<(T, Situacao), ErroDoReconhecimento>;

/// Roda uma etapa: confere o cancelamento, anuncia `Rodando` e depois o desfecho.
fn etapa<T>(
    andamento: &mut dyn FnMut(Andamento),
    cancelar: &AtomicBool,
    qual: Etapa,
    trabalho: impl FnOnce() -> Saida<T>,
) -> Result<T, ErroDoReconhecimento> {
    if cancelar.load(Ordering::SeqCst) {
        return Err(ErroDoReconhecimento::Cancelado);
    }
    andamento(Andamento {
        etapa: qual,
        situacao: Situacao::Rodando,
    });
    match trabalho() {
        Ok((valor, situacao)) => {
            andamento(Andamento {
                etapa: qual,
                situacao,
            });
            Ok(valor)
        }
        Err(e) => {
            andamento(Andamento {
                etapa: qual,
                situacao: Situacao::Falhou(e.to_string()),
            });
            Err(e)
        }
    }
}

impl Reconhecedor for ReconhecedorDoCofre {
    fn reconhecer(
        &self,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        let meta = std::fs::metadata(caminho).map_err(|e| do_arquivo(&e))?;
        if !meta.is_file() {
            return Err(ErroDoReconhecimento::Arquivo(
                "o que foi solto não é um arquivo".to_owned(),
            ));
        }
        let nome = caminho
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let banco_do_cofre = self.cofre.join("inventario.sqlite");
        let mut estado = Estado {
            sha256: String::new(),
            ja_existia: false,
            dentro_do_cofre: false,
            sem_achado: false,
            texto: EstadoDoTexto::SemTexto("não houve tentativa de extrair o texto".to_owned()),
            conteudo: None,
            reconhecido: Reconhecido::Nada {
                motivo: "ainda sem leitura".to_owned(),
            },
            sugestoes: Vec::new(),
            avisos: Vec::new(),
        };

        // 1. Conferir.
        let conn = etapa(andamento, cancelar, Etapa::Conferir, || {
            let conn = abrir_escrita(&banco_do_cofre)?;
            estado.sha256 = sha256_do_arquivo(caminho).map_err(|e| do_arquivo(&e))?;
            estado.dentro_do_cofre = esta_dentro(caminho, &self.cofre);
            estado.ja_existia = achado_existe(&conn, &estado.sha256)?;
            Ok((conn, Situacao::Feita))
        })?;
        estado.sem_achado = !estado.ja_existia && estado.dentro_do_cofre;

        // 2. Registrar.
        etapa(andamento, cancelar, Etapa::Registrar, || {
            if estado.dentro_do_cofre {
                return Ok((
                    (),
                    Situacao::Pulada("o arquivo já está no cofre".to_owned()),
                ));
            }
            let r = registrar_arrastado(&conn, &self.cofre, caminho, &estado.sha256)
                .map_err(da_coleta)?;
            if r.caminho_acervo.is_none() {
                if meta.len() > sbf_coleta::pastas::LIMITE_COPIA_BYTES {
                    estado.avisos.push(
                        "o arquivo é grande demais para copiar ao acervo; o original segue onde está"
                            .to_owned(),
                    );
                } else {
                    return Ok((
                        (),
                        Situacao::Falhou("a cópia ao acervo do cofre falhou".to_owned()),
                    ));
                }
            }
            let situacao = if estado.ja_existia && !r.copiado_agora {
                Situacao::Pulada("o inventário já tinha o arquivo".to_owned())
            } else {
                Situacao::Feita
            };
            Ok(((), situacao))
        })?;

        // 3. Extrair o texto.
        etapa(andamento, cancelar, Etapa::ExtrairTexto, || {
            let (texto, conteudo, situacao) = self.extrair_texto(&conn, caminho, &estado)?;
            estado.texto = texto;
            estado.conteudo = conteudo;
            Ok(((), situacao))
        })?;

        // 4. Leitores locais.
        etapa(andamento, cancelar, Etapa::LerLocalmente, || {
            let situacao = self.ler_localmente(&mut estado);
            Ok(((), situacao))
        })?;

        // 5. IA, só quando os leitores locais não reconheceram.
        etapa(andamento, cancelar, Etapa::PerguntarAIa, || {
            let situacao = self.perguntar_a_ia(&conn, &banco_do_cofre, cancelar, &mut estado)?;
            Ok(((), situacao))
        })?;

        // 6. Sugerir.
        etapa(andamento, cancelar, Etapa::Sugerir, || {
            let situacao = self.sugerir(&mut estado);
            Ok(((), situacao))
        })?;

        Ok(Resultado {
            sha256: estado.sha256,
            nome,
            ja_existia: estado.ja_existia,
            texto: estado.texto,
            reconhecido: estado.reconhecido,
            sugestoes: estado.sugestoes,
            avisos: estado.avisos,
        })
    }
}

fn abrir_escrita(caminho: &Path) -> Result<Connection, ErroDoReconhecimento> {
    if !caminho.is_file() {
        return Err(ErroDoReconhecimento::Banco(
            "o cofre não tem inventario.sqlite".to_owned(),
        ));
    }
    let conn = Connection::open_with_flags(
        caminho,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(banco)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(banco)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(banco)?;
    Ok(conn)
}

fn achado_existe(conn: &Connection, sha256: &str) -> Result<bool, ErroDoReconhecimento> {
    conn.query_row(
        "SELECT 1 FROM achado WHERE content_hash = ?1",
        [sha256],
        |_| Ok(()),
    )
    .optional()
    .map(|l| l.is_some())
    .map_err(banco)
}

/// O arquivo está dentro da pasta do cofre (depois de resolver links e `..`)?
fn esta_dentro(arquivo: &Path, cofre: &Path) -> bool {
    match (arquivo.canonicalize(), cofre.canonicalize()) {
        (Ok(a), Ok(c)) => a.starts_with(c),
        _ => false,
    }
}

fn extensao(caminho: &Path) -> String {
    caminho
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

impl ReconhecedorDoCofre {
    fn extrair_texto(
        &self,
        conn: &Connection,
        caminho: &Path,
        estado: &Estado,
    ) -> Result<(EstadoDoTexto, Option<String>, Situacao), ErroDoReconhecimento> {
        let sha = &estado.sha256;
        if estado.sem_achado {
            let motivo = "o arquivo está no cofre, mas o inventário não o conhece".to_owned();
            return Ok((
                EstadoDoTexto::SemTexto(motivo.clone()),
                None,
                Situacao::Pulada(motivo),
            ));
        }
        if let Some(t) = texto::ler_texto(conn, sha).map_err(banco)? {
            let conteudo = (!t.conteudo.trim().is_empty()).then_some(t.conteudo);
            return Ok((
                EstadoDoTexto::JaHavia,
                conteudo,
                Situacao::Pulada("o inventário já tinha o texto".to_owned()),
            ));
        }
        match extensao(caminho).as_str() {
            "pdf" => self.extrair_do_pdf(conn, caminho, sha),
            "txt" => extrair_do_txt(conn, caminho, sha),
            _ => {
                let motivo = "ainda sem leitor de texto para este formato".to_owned();
                Ok((
                    EstadoDoTexto::SemTexto(motivo.clone()),
                    None,
                    Situacao::Pulada(motivo),
                ))
            }
        }
    }

    fn extrair_do_pdf(
        &self,
        conn: &Connection,
        caminho: &Path,
        sha: &str,
    ) -> Result<(EstadoDoTexto, Option<String>, Situacao), ErroDoReconhecimento> {
        let sem = |motivo: &str, situacao: Situacao| {
            Ok((EstadoDoTexto::SemTexto(motivo.to_owned()), None, situacao))
        };
        let pdf = match PdfProtegido::ler(caminho) {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::FileTooLarge => {
                let m = "o PDF passa do limite de leitura";
                return sem(m, Situacao::Pulada(m.to_owned()));
            }
            Err(_) => {
                let m = "não deu para ler o PDF";
                return sem(m, Situacao::Falhou(m.to_owned()));
            }
        };
        if pdf.precisa_de_senha() {
            return self.abrir_protegido(conn, sha);
        }
        match pdf.abrir_sem_senha(&self.ferramentas) {
            Desfecho::Aberto(t) => {
                let conteudo = cortar_como_o_js(&t.texto, LIMITE_DO_TEXTO);
                texto::registrar_texto(
                    conn,
                    sha,
                    sbf_inventario::protegidos::METODO_PDFTOTEXT,
                    &conteudo,
                    Some(&impressao_canonica(&t.texto)),
                )
                .map_err(banco)?;
                Ok((EstadoDoTexto::Extraido, Some(t.texto), Situacao::Feita))
            }
            Desfecho::SemTexto { .. } => {
                texto::registrar_texto(
                    conn,
                    sha,
                    sbf_inventario::protegidos::METODO_SEM_TEXTO,
                    "",
                    None,
                )
                .map_err(banco)?;
                let m = "o PDF não tem texto (imagem): candidato a OCR";
                sem(m, Situacao::Pulada(m.to_owned()))
            }
            Desfecho::FalhaNaExtracao(_) => {
                let m = "o pdftotext não leu o PDF";
                sem(m, Situacao::Falhou(m.to_owned()))
            }
            _ => {
                let m = "o PDF não é legível";
                sem(m, Situacao::Falhou(m.to_owned()))
            }
        }
    }

    /// PDF que precisa de senha: marca o achado como protegido e roda a rodada do chaveiro
    /// só para ele (as regras de senha, o limite de falhas e a pendência são os de sempre).
    fn abrir_protegido(
        &self,
        conn: &Connection,
        sha: &str,
    ) -> Result<(EstadoDoTexto, Option<String>, Situacao), ErroDoReconhecimento> {
        use sbf_pdf::Situacao as Sit;
        conn.execute(
            "UPDATE achado SET protegido_senha = 1
              WHERE content_hash = ?1 AND protegido_senha IS NOT 1",
            [sha],
        )
        .map_err(banco)?;
        let opcoes = OpcoesDaRodada {
            so_os_achados: Some(BTreeSet::from([sha.to_owned()])),
            ferramentas: self.ferramentas.clone(),
            ..OpcoesDaRodada::default()
        };
        let relatorio = sbf_pdf::abrir_protegidos(conn, &self.cofre, &*self.segredos, &opcoes)
            .map_err(banco)?;
        let protegido = |motivo: String| {
            Ok((
                EstadoDoTexto::Protegido(motivo.clone()),
                None,
                Situacao::Falhou(motivo),
            ))
        };
        match relatorio.resultados.first().map(|r| &r.situacao) {
            Some(Sit::Aberto { .. }) => {
                let t = texto::ler_texto(conn, sha).map_err(banco)?;
                Ok((
                    EstadoDoTexto::Extraido,
                    t.map(|t| t.conteudo).filter(|c| !c.trim().is_empty()),
                    Situacao::Feita,
                ))
            }
            Some(Sit::SemTexto { .. }) => {
                let m = "o PDF não tem texto (imagem): candidato a OCR".to_owned();
                Ok((
                    EstadoDoTexto::SemTexto(m.clone()),
                    None,
                    Situacao::Pulada(m),
                ))
            }
            Some(Sit::Pendencia { .. }) => protegido(
                "PDF protegido: nenhuma regra abriu e a pendência de PDF protegido foi aberta"
                    .to_owned(),
            ),
            Some(Sit::SemRegra) => {
                protegido("PDF protegido sem regra de senha para este fornecedor".to_owned())
            }
            Some(Sit::NaoAbriu) => protegido("nenhuma regra de senha abriu o PDF".to_owned()),
            Some(Sit::FalhaNaExtracao) => {
                protegido("a senha conferiu, mas a extração do texto falhou".to_owned())
            }
            Some(Sit::FaltouSegredo(e)) => protegido(format!("PDF protegido: {e}")),
            Some(Sit::SemArquivo) => {
                protegido("o PDF protegido não foi encontrado para abrir".to_owned())
            }
            Some(Sit::GrandeDemais) => {
                let m = "o PDF passa do limite de leitura".to_owned();
                Ok((
                    EstadoDoTexto::SemTexto(m.clone()),
                    None,
                    Situacao::Pulada(m),
                ))
            }
            None => protegido(
                "este PDF protegido já tem pendência aberta ou passou do limite de tentativas"
                    .to_owned(),
            ),
        }
    }

    fn ler_localmente(&self, estado: &mut Estado) -> Situacao {
        let Some(conteudo) = estado.conteudo.as_deref() else {
            estado.reconhecido = Reconhecido::Nada {
                motivo: "o arquivo não tem texto para ler".to_owned(),
            };
            return Situacao::Pulada("sem texto para os leitores locais".to_owned());
        };
        let parece_contracheque =
            sbf_planilha::texto::normalizar(conteudo).contains("demonstrativo de pagamento");
        if !parece_contracheque {
            estado.reconhecido = Reconhecido::Nada {
                motivo: "nenhum leitor local reconheceu o documento".to_owned(),
            };
            return Situacao::Feita;
        }
        match sbf_planilha::contracheque::ler(conteudo) {
            Ok(c) => {
                if !c.conferido {
                    estado.avisos.push(
                        "a soma das rubricas não bate com o líquido do contracheque".to_owned(),
                    );
                }
                estado.avisos.extend(c.avisos.iter().cloned());
                estado.reconhecido = Reconhecido::Contracheque {
                    consignacoes: c.consignacoes_com_prazo(),
                    contracheque: c,
                };
                Situacao::Feita
            }
            Err(_) => {
                estado.reconhecido = Reconhecido::Nada {
                    motivo: "o demonstrativo de pagamento não pôde ser lido".to_owned(),
                };
                Situacao::Falhou("o demonstrativo de pagamento não pôde ser lido".to_owned())
            }
        }
    }

    fn perguntar_a_ia(
        &self,
        conn: &Connection,
        banco_do_cofre: &Path,
        cancelar: &AtomicBool,
        estado: &mut Estado,
    ) -> Result<Situacao, ErroDoReconhecimento> {
        if !matches!(estado.reconhecido, Reconhecido::Nada { .. }) {
            return Ok(Situacao::Pulada(
                "os leitores locais reconheceram".to_owned(),
            ));
        }
        if estado.conteudo.is_none() {
            estado.reconhecido = Reconhecido::Nada {
                motivo: "o arquivo não tem texto para a IA".to_owned(),
            };
            return Ok(Situacao::Pulada("sem texto para enviar à IA".to_owned()));
        }
        let motor = match &self.ia {
            Ia::Nenhuma => {
                estado.reconhecido = Reconhecido::Nada {
                    motivo: "nenhum leitor local reconheceu e a IA não está configurada".to_owned(),
                };
                return Ok(Situacao::Pulada("a IA não está configurada".to_owned()));
            }
            Ia::Motor(m) => m.clone(),
            Ia::Provedor(p) => montar_motor(banco_do_cofre, p)?,
        };
        let alvo = Alvo::Achado(estado.sha256.clone());
        // Uma sugestão aberta que já existe vale: não gasta uma segunda chamada.
        let mut abertas = sugestoes_abertas(conn, &estado.sha256)?;
        let mut situacao = Situacao::Pulada("a IA já tinha sugerido para este arquivo".to_owned());
        if abertas.is_empty() {
            let controle = Controle::novo();
            let progresso = Mutex::new(Progresso::ocioso());
            let config = ConfigDaRodada {
                rastro: motor.rastro.clone(),
                ..ConfigDaRodada::default()
            };
            let fim = AtomicBool::new(false);
            let resultado = std::thread::scope(|escopo| {
                escopo.spawn(|| {
                    while !fim.load(Ordering::SeqCst) {
                        if cancelar.load(Ordering::SeqCst) {
                            controle.cancelar();
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(40));
                    }
                });
                let _soltar = SoltarAoSair(&fim);
                rodar(
                    conn,
                    &motor.fila,
                    motor.provedor,
                    std::slice::from_ref(&alvo),
                    &config,
                    &controle,
                    &progresso,
                    &|id| motor.registro.definir_rodada(Some(id)),
                )
            });
            motor.registro.definir_rodada(None);
            if cancelar.load(Ordering::SeqCst) {
                return Err(ErroDoReconhecimento::Cancelado);
            }
            let p = resultado.map_err(banco)?;
            abertas = sugestoes_abertas(conn, &estado.sha256)?;
            situacao = match p.situacao {
                SituacaoDaRodada::Concluida => Situacao::Feita,
                _ => Situacao::Falhou(
                    p.aviso
                        .unwrap_or_else(|| "a IA não terminou a consulta".to_owned()),
                ),
            };
        }
        estado.reconhecido = resumir_a_sugestao_da_ia(&abertas);
        Ok(situacao)
    }

    fn sugerir(&self, estado: &mut Estado) -> Situacao {
        let Reconhecido::Contracheque {
            contracheque: c, ..
        } = &estado.reconhecido
        else {
            return Situacao::Pulada(
                "não há o que ligar no planejamento para este tipo de arquivo".to_owned(),
            );
        };
        let caminho = self.cofre.join("planejamento.sqlite");
        if !caminho.is_file() {
            return Situacao::Pulada("o cofre ainda não tem planejamento".to_owned());
        }
        let plan = match Banco::abrir(&caminho) {
            Ok(b) => PlanejamentoSqlite::novo(b, Box::new(SemSaldos)),
            Err(_) => return Situacao::Falhou("não deu para abrir o planejamento".to_owned()),
        };
        let recorrente = match recorrente_do_salario(&plan, None) {
            Ok(r) => r,
            Err(ErroDoSalario::Nenhum) => {
                estado.avisos.push(
                    "não achei o recorrente do salário no planejamento; ligue o contracheque a ele à mão"
                        .to_owned(),
                );
                return Situacao::Feita;
            }
            Err(ErroDoSalario::Varios) => {
                estado.avisos.push(
                    "há mais de um recorrente de salário no planejamento; ligue o contracheque ao certo à mão"
                        .to_owned(),
                );
                return Situacao::Feita;
            }
            Err(_) => return Situacao::Falhou("não deu para ler o planejamento".to_owned()),
        };
        let mes = format!("{:02}/{}", c.pagamento.mes, c.pagamento.ano);
        let alvo = ocorrencia_do_mes(&recorrente.id, c.pagamento);
        let uso = UsoDeRubricas {
            recorrente: recorrente.id.clone(),
            nome: recorrente.nome.clone(),
            documento: estado.sha256.clone(),
            contracheque: c.clone(),
            digitado_no_mes: valor_digitado(&recorrente, c.pagamento),
            digitado_depois: valor_digitado(&recorrente, c.pagamento.somar_meses(1)),
        };
        let marcacao = uso.eventuais_iniciais();
        let composicoes = uso.composicoes(&marcacao);
        let vinculado = ja_vinculado(&plan, &alvo, &estado.sha256);
        let vigentes: Vec<_> = composicoes
            .iter()
            .map(|k| composicao_ja_vigente(&plan, k))
            .collect();
        let (Ok(vinculado), Ok(vigentes)) = (
            vinculado,
            vigentes.into_iter().collect::<Result<Vec<bool>, _>>(),
        ) else {
            return Situacao::Falhou("não deu para ler o planejamento".to_owned());
        };
        let mut sugestoes = Vec::new();
        if !vinculado {
            sugestoes.push(Sugestao::Vincular {
                alvo,
                rotulo: format!("Vincular ao {} · {mes}", recorrente.nome),
                tipo: TipoDeDocumento::Contracheque,
                data: Some(c.pagamento.dia(1)),
            });
        }
        if !vigentes.iter().all(|v| *v) {
            sugestoes.push(Sugestao::UsarRubricas {
                rotulo: uso.rotulo(&marcacao),
                uso: Box::new(uso),
            });
        }
        if let (Some(credito), Some(liquido)) = (c.credito, c.liquido) {
            // O Salário do mês ainda sem pagamento informado nem conciliação.
            match plan.proximas(&recorrente.id, c.pagamento, 1, credito) {
                Ok(ocorrencias) => {
                    if let Some(o) = ocorrencias.iter().find(|o| {
                        o.id.competencia == c.pagamento
                            && o.pago_informado.is_none()
                            && o.conciliado.is_none()
                    }) {
                        sugestoes.push(Sugestao::InformarRecebido {
                            rotulo: format!(
                                "Informar recebido em {:02}/{:02} pelo líquido",
                                credito.dia, credito.mes
                            ),
                            ocorrencia: o.id.clone(),
                            valor: liquido,
                            data: credito,
                        });
                    }
                }
                Err(_) => return Situacao::Falhou("não deu para ler o planejamento".to_owned()),
            }
        }
        estado.sugestoes = sugestoes;
        Situacao::Feita
    }
}

struct SoltarAoSair<'a>(&'a AtomicBool);

impl Drop for SoltarAoSair<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn extrair_do_txt(
    conn: &Connection,
    caminho: &Path,
    sha: &str,
) -> Result<(EstadoDoTexto, Option<String>, Situacao), ErroDoReconhecimento> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(caminho)
        .and_then(|f| f.take(LIMITE_DO_TXT).read_to_end(&mut bytes))
        .map_err(|e| do_arquivo(&e))?;
    let texto_inteiro = String::from_utf8_lossy(&bytes).into_owned();
    if texto_inteiro.trim().is_empty() {
        let m = "o arquivo de texto está vazio".to_owned();
        return Ok((
            EstadoDoTexto::SemTexto(m.clone()),
            None,
            Situacao::Pulada(m),
        ));
    }
    texto::registrar_texto(
        conn,
        sha,
        METODO_TEXTO_SIMPLES,
        &cortar_como_o_js(&texto_inteiro, LIMITE_DO_TEXTO),
        Some(&impressao_canonica(&texto_inteiro)),
    )
    .map_err(banco)?;
    Ok((
        EstadoDoTexto::Extraido,
        Some(texto_inteiro),
        Situacao::Feita,
    ))
}

/// A fila da triagem com o provedor dado: o prompt e o nível de confiança são os de
/// `configuracao_da_fila` (os da área Triagem).
fn montar_motor(
    banco_do_cofre: &Path,
    provedor: &Arc<dyn Provedor>,
) -> Result<MotorDeIa, ErroDoReconhecimento> {
    let registro = Arc::new(RegistroDaIa::abrir(banco_do_cofre, None).map_err(banco)?);
    let rastro = RastroDeFalhas::default();
    let fila = Arc::new(Fila::nova(
        rastro.embrulhar(Arc::clone(provedor)),
        None,
        Arc::clone(&registro) as Arc<dyn Registro>,
        configuracao_da_fila(false),
    ));
    Ok(MotorDeIa {
        fila,
        registro,
        provedor: provedor.nome(),
        rastro: Some(rastro),
    })
}

/// Uma linha aberta de `sugestao_ia` do achado.
struct SugestaoAberta {
    id: i64,
    campo: String,
    valor: serde_json::Value,
}

fn sugestoes_abertas(
    conn: &Connection,
    sha: &str,
) -> Result<Vec<SugestaoAberta>, ErroDoReconhecimento> {
    let mut consulta = conn
        .prepare(
            "SELECT id, campo, valor_json FROM sugestao_ia
              WHERE alvo_tipo = 'achado' AND alvo_id = ?1 AND estado = 'aberta'
                AND campo <> ?2
              ORDER BY id",
        )
        .map_err(banco)?;
    let linhas = consulta
        .query_map(
            rusqlite::params![sha, sbf_inventario::sugestao::CAMPO_NAO_CLASSIFICADO],
            |l| {
                let texto: Option<String> = l.get(2)?;
                Ok(SugestaoAberta {
                    id: l.get(0)?,
                    campo: l.get(1)?,
                    valor: texto
                        .and_then(|t| serde_json::from_str(&t).ok())
                        .unwrap_or(serde_json::Value::Null),
                })
            },
        )
        .map_err(banco)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(banco)?;
    Ok(linhas)
}

fn rotulo_do_tipo(tipo: &str) -> &'static str {
    match tipo {
        "fatura" => "Fatura",
        "boleto" => "Boleto",
        "extrato" => "Extrato",
        "comprovante" => "Comprovante",
        "nota_fiscal" => "Nota fiscal",
        "contracheque" => "Contracheque",
        "informe_rendimentos" => "Informe de rendimentos",
        "contrato" => "Contrato",
        "recibo" => "Recibo",
        _ => "Documento",
    }
}

/// `AAAA-MM-DD` → `DD/MM/AAAA`.
fn data_brasileira(iso: &str) -> Option<String> {
    let mut p = iso.split('-');
    let (a, m, d) = (p.next()?, p.next()?, p.next()?);
    (a.len() == 4 && m.len() == 2 && d.len() == 2 && p.next().is_none())
        .then(|| format!("{d}/{m}/{a}"))
}

/// A linha do cartão: tipo, fornecedor e data (nada além disso).
fn resumir_a_sugestao_da_ia(abertas: &[SugestaoAberta]) -> Reconhecido {
    let texto_de = |campo: &str| {
        abertas
            .iter()
            .find(|s| s.campo == campo)
            .and_then(|s| s.valor.as_str())
            .map(str::to_owned)
    };
    let tipo = texto_de("tipo_documento");
    let fornecedor = texto_de("fornecedor");
    let data = texto_de("data_chave").and_then(|d| data_brasileira(&d));
    if tipo.is_none() && fornecedor.is_none() && data.is_none() {
        let nao_financeiro = texto_de("e_financeiro").as_deref() == Some("nao");
        return Reconhecido::Nada {
            motivo: if nao_financeiro {
                "a IA entende que não é um documento financeiro".to_owned()
            } else {
                "a IA não conseguiu classificar este arquivo".to_owned()
            },
        };
    }
    let id_da_sugestao = abertas
        .iter()
        .find(|s| s.campo == "tipo_documento")
        .or_else(|| abertas.first())
        .map_or(0, |s| s.id);
    let mut partes = Vec::new();
    partes.push(
        tipo.as_deref()
            .map_or("Documento", rotulo_do_tipo)
            .to_owned(),
    );
    if let Some(f) = fornecedor {
        partes.push(f);
    }
    if let Some(d) = data {
        partes.push(format!("data {d}"));
    }
    Reconhecido::PelaIa {
        id_da_sugestao,
        resumo: partes.join(" · "),
    }
}
