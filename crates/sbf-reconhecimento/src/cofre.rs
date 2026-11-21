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
use sbf_inventario::caixa_de_entrada::{self as caixa, Canal, Chegada, Estado as EstadoDaCaixa};
use sbf_inventario::impressao_do_texto;
use sbf_inventario::sugestao::RegistroDaIa;
use sbf_inventario::texto::{self, METODO_TEXTO_SIMPLES};
use sbf_ocr::{DesfechoDoOcr, FalhaDoOcr, Ocr};
use sbf_pdf::texto::{LIMITE_DO_TEXTO, cortar_como_o_js, impressao_canonica};
use sbf_pdf::{Desfecho, Ferramentas, FonteDeSegredos, OpcoesDaRodada, PdfProtegido};
use sbf_planejamento::{
    Banco, Centavos, Competencia, Conciliacao, Data, Planejamento, PlanejamentoSqlite, Recorrente,
    SemSaldos, TipoDeDocumento,
};
use sbf_planilha::movimentos::MovimentosDaPluggy;
use sbf_triagem::Alvo;
use sbf_triagem::rodada::{
    ConfigDaRodada, Progresso, RastroDeFalhas, SituacaoDaRodada, configuracao_da_fila, rodar,
};

use crate::descontos::{propor_descontos, sugestao_de_baixa};
use crate::regras::{self, Ligadas};
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
    pub(crate) cofre: PathBuf,
    segredos: Segredos,
    ia: Ia,
    ferramentas: Ferramentas,
    /// O tesseract das imagens (PNG, JPG, WEBP).
    ocr: Ocr,
    /// As regras de reconhecimento (spec 13); `None` é o caminho de sempre.
    pub(crate) regras: Option<Ligadas>,
    /// Como o reconhecimento mexe na Caixa de entrada; `None` não toca nela (o cartão que a
    /// janela recalcula, os testes sem caixa).
    na_caixa: Option<NaCaixa>,
    /// Só lê o texto que o inventário já guarda; sem ele não extrai (nem OCR, nem senha):
    /// o cartão da Caixa de entrada roda na thread da interface.
    so_texto_guardado: bool,
}

/// Como o reconhecimento registra o arquivo na Caixa de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaCaixa {
    /// Chegou agora: cria a linha (ou atualiza a que existe), com o canal e a regra de
    /// reabertura da [`Chegada`].
    Registrar(Chegada),
    /// "Reconhecer de novo": só atualiza a linha que já existe, sem reabrir o que o usuário
    /// decidiu e sem criar linha.
    Atualizar,
}

/// O conteúdo de que o reconhecimento já sabe o sha256 (para a falha não deixar a linha da
/// Caixa de entrada em Processando).
#[derive(Debug, Clone)]
struct ConteudoVisto {
    sha256: String,
    nome: String,
}

impl std::fmt::Debug for ReconhecedorDoCofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReconhecedorDoCofre")
            .field("cofre", &self.cofre)
            .field("ia", &!matches!(self.ia, Ia::Nenhuma))
            .field("regras", &self.regras.is_some())
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
            ocr: Ocr::do_sistema(),
            regras: None,
            na_caixa: None,
            so_texto_guardado: false,
        }
    }

    /// Registra cada arquivo reconhecido na Caixa de entrada, chegado por este canal.
    /// `reabrir`: o que o usuário já tinha resolvido ou dispensado volta a abrir (só uma ação
    /// dele, como `sbf reconhecer`, pede isso). Sem esta chamada, e sem canal em
    /// [`ReconhecedorDoCofre::reconhecer_de`], nada entra na Caixa de entrada.
    #[must_use]
    pub fn com_caixa_de_entrada(mut self, canal: Canal, reabrir: bool) -> Self {
        self.na_caixa = Some(NaCaixa::Registrar(Chegada { canal, reabrir }));
        self
    }

    /// "Reconhecer de novo": cada reconhecimento só atualiza a linha que a Caixa de entrada já
    /// tem (tipo, resumo, estado), sem criá-la e sem reabrir o que o usuário decidiu.
    #[must_use]
    pub fn atualizando_a_caixa_de_entrada(mut self) -> Self {
        self.na_caixa = Some(NaCaixa::Atualizar);
        self
    }

    /// Só usa o texto que o inventário já guarda: sem ele, o arquivo fica "sem texto
    /// guardado" e nada é extraído (nem OCR, nem a rodada de senhas). Serve ao que roda na
    /// thread da interface.
    #[must_use]
    pub fn so_com_texto_guardado(mut self) -> Self {
        self.so_texto_guardado = true;
        self
    }

    /// Liga o motor de regras de reconhecimento (spec 13, ADR-021) ao passo de leitura local:
    /// abre `<cofre>/regras.sqlite` (criando e semeando as regras de fábrica) e consulta as
    /// versões ativas e consolidadas antes dos leitores nativos. `executor` roda os scripts
    /// (`sbf_script::ExecutorNoProcesso`); sem ele, regras com script falham sem derrubar nada.
    ///
    /// Sem esta chamada, ou se o catálogo não abrir, o caminho atual roda exatamente como
    /// antes (AC-LUA-14).
    #[must_use]
    pub fn com_regras(
        mut self,
        executor: Option<Arc<dyn sbf_regras::script::ExecutorDeScript>>,
    ) -> Self {
        self.regras = Some(Ligadas { executor });
        self
    }

    /// Usa o motor da triagem (a mesma fila, reserva e registro da área Triagem) em vez de
    /// montar uma fila só com o provedor.
    #[must_use]
    pub fn com_motor_de_ia(mut self, motor: MotorDeIa) -> Self {
        self.ia = Ia::Motor(motor);
        self
    }

    /// O tesseract das imagens (Opções; os testes). O padrão é o do sistema.
    #[must_use]
    pub fn com_ocr(mut self, ocr: Ocr) -> Self {
        self.ocr = ocr;
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
pub(crate) struct Estado {
    pub(crate) sha256: String,
    pub(crate) ja_existia: bool,
    pub(crate) dentro_do_cofre: bool,
    /// O achado não está no inventário (arquivo de dentro do cofre que ele não conhece).
    pub(crate) sem_achado: bool,
    pub(crate) texto: EstadoDoTexto,
    /// O texto para os leitores e a IA; `None` quando não há.
    pub(crate) conteudo: Option<String>,
    pub(crate) reconhecido: Reconhecido,
    pub(crate) sugestoes: Vec<Sugestao>,
    pub(crate) avisos: Vec<String>,
    /// O reconhecimento da regra (ou o do contracheque já completado), com as propostas.
    pub(crate) regra: Option<sbf_regras::Reconhecimento>,
    pub(crate) entrada: Option<sbf_regras::Entrada>,
    pub(crate) leitura: Option<sbf_regras::Leitura>,
    /// O canal de onde o arquivo veio e o id dele no canal (`Canal::Gmail` e o id da
    /// mensagem). `None`: solto na janela ou já no cofre, como sempre.
    pub(crate) origem: Option<(sbf_regras::Canal, String)>,
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
        self.reconhecer_de(None, caminho, andamento, cancelar)
    }
}

impl ReconhecedorDoCofre {
    /// Como [`Reconhecedor::reconhecer`], mas dizendo de que canal o arquivo veio (o Gmail, por
    /// exemplo) e o id dele no canal: é o que as regras de reconhecimento veem em
    /// `Entrada.origem`. `None` é o comportamento de sempre (solto na janela ou já no cofre).
    ///
    /// Um arquivo de dentro do cofre só segue as etapas de texto se o inventário já o
    /// conhece: quem entrega por um canal registra o achado antes de chamar.
    ///
    /// # Errors
    /// [`ErroDoReconhecimento`], como no trait.
    pub fn reconhecer_de(
        &self,
        origem: Option<(sbf_regras::Canal, String)>,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        let na_caixa = self.na_caixa.or_else(|| {
            origem
                .as_ref()
                .and_then(|(canal, _)| crate::caixa_de_entrada::canal_da_caixa(*canal))
                .map(|canal| {
                    NaCaixa::Registrar(Chegada {
                        canal,
                        reabrir: false,
                    })
                })
        });
        self.reconhecer_registrando(origem, caminho, andamento, cancelar, na_caixa)
    }

    /// Como [`ReconhecedorDoCofre::reconhecer`], registrando o arquivo na Caixa de entrada
    /// com a `chegada` dada (o canal varia por arquivo: a pasta do celular e as outras).
    ///
    /// # Errors
    /// [`ErroDoReconhecimento`], como no trait.
    pub fn reconhecer_chegada(
        &self,
        chegada: Chegada,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        self.reconhecer_registrando(
            None,
            caminho,
            andamento,
            cancelar,
            Some(NaCaixa::Registrar(chegada)),
        )
    }

    /// A leitura de um conteúdo falhou antes de o reconhecimento saber o que ele é (o arquivo
    /// sumiu, não abriu): a Caixa de entrada guarda isso como "Precisa revisar", com o pedido
    /// de reconhecer de novo, em vez de deixá-lo cair em "sem padrão de reconhecimento" ou
    /// sumir.
    pub fn registrar_falha_da_leitura(&self, chegada: Chegada, sha256: &str, nome: &str) {
        if let Ok(conn) = abrir_escrita(&self.cofre.join("inventario.sqlite")) {
            let visto = ConteudoVisto {
                sha256: sha256.to_owned(),
                nome: nome.to_owned(),
            };
            registrar_falha(&conn, NaCaixa::Registrar(chegada), &visto);
        }
    }

    fn reconhecer_registrando(
        &self,
        origem: Option<(sbf_regras::Canal, String)>,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
        na_caixa: Option<NaCaixa>,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        let mut visto: Option<ConteudoVisto> = None;
        let r = self.executar(origem, caminho, andamento, cancelar, na_caixa, &mut visto);
        // O reconhecimento parou depois de saber qual era o conteúdo: a Caixa de entrada não
        // fica com o "Processando" para sempre, e a falha vai para "Precisa revisar".
        if let (Err(_), Some(na_caixa), Some(visto)) = (&r, na_caixa, &visto)
            && let Ok(conn) = abrir_escrita(&self.cofre.join("inventario.sqlite"))
        {
            registrar_falha(&conn, na_caixa, visto);
        }
        r
    }

    /// Reconhece de novo um conteúdo que o inventário já tem, sem o arquivo na mão: lê a cópia
    /// do acervo (ou, sem ela, uma origem local que ainda exista) e roda as mesmas etapas. O
    /// texto já guardado vale (não extrai de novo), então é rápido; é assim que a Caixa de
    /// entrada recalcula as sugestões e que "Reconhecer de novo" reaproveita as regras novas.
    ///
    /// # Errors
    /// [`ErroDoReconhecimento::Arquivo`] quando o conteúdo não tem arquivo para ler.
    pub fn reconhecer_conteudo(
        &self,
        sha256: &str,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
    ) -> Result<Resultado, ErroDoReconhecimento> {
        let caminho = self.arquivo_do_conteudo(sha256)?;
        self.reconhecer(&caminho, andamento, cancelar)
    }

    /// O arquivo que guarda o conteúdo: a cópia do acervo ou uma origem local que existe.
    ///
    /// # Errors
    /// Sem arquivo, ou o banco não abre.
    pub fn arquivo_do_conteudo(&self, sha256: &str) -> Result<PathBuf, ErroDoReconhecimento> {
        let conn = Connection::open_with_flags(
            self.cofre.join("inventario.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(banco)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(banco)?;
        if let Some(rel) = caixa::caminho_no_acervo(&conn, sha256).map_err(banco)? {
            let p = self.cofre.join(rel);
            if p.is_file() {
                return Ok(p);
            }
        }
        let mut q = conn
            .prepare(
                "SELECT local_filepath FROM origem
                  WHERE content_hash = ?1 AND local_filepath IS NOT NULL
                  ORDER BY id DESC",
            )
            .map_err(banco)?;
        let caminhos = q
            .query_map([sha256], |l| l.get::<_, String>(0))
            .map_err(banco)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(banco)?;
        caminhos
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or_else(|| {
                ErroDoReconhecimento::Arquivo("o arquivo não está mais no cofre".to_owned())
            })
    }

    fn executar(
        &self,
        origem: Option<(sbf_regras::Canal, String)>,
        caminho: &Path,
        andamento: &mut dyn FnMut(Andamento),
        cancelar: &AtomicBool,
        na_caixa: Option<NaCaixa>,
        visto: &mut Option<ConteudoVisto>,
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
            regra: None,
            entrada: None,
            leitura: None,
            origem,
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
        if let Some(na_caixa) = na_caixa {
            *visto = Some(ConteudoVisto {
                sha256: estado.sha256.clone(),
                nome: nome.clone(),
            });
            if let NaCaixa::Registrar(c) = na_caixa {
                let _ = caixa::marcar_processando(&conn, &estado.sha256, &nome, c.canal);
            }
        }

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
            let situacao = self.ler_localmente(&conn, &mut estado, caminho);
            if let Reconhecido::Contracheque {
                contracheque: c, ..
            } = &estado.reconhecido
            {
                // D1 da spec 12: o contracheque declara o registro `salario` (uma vez só).
                let data = c.credito.unwrap_or_else(|| c.pagamento.dia(1));
                let rubricas = c
                    .rubricas
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !c.informativas.get(*i).copied().unwrap_or(false))
                    .map(|(i, r)| sbf_inventario::salario::RubricaDoContracheque {
                        codigo: r.codigo.clone(),
                        descricao: r.nome.clone(),
                        valor_centavos: r.valor.0,
                        desconto: r.tipo == sbf_planejamento::TipoDeRubrica::Desconto,
                        parcela: c.parcelas_atuais.get(i).copied().flatten(),
                    })
                    .collect();
                let salario = sbf_inventario::salario::SalarioDoContracheque {
                    sha256: estado.sha256.clone(),
                    data: format!("{:04}-{:02}-{:02}", data.ano, data.mes, data.dia),
                    liquido_centavos: c.liquido_do_mes().0,
                    rubricas,
                };
                if sbf_inventario::salario::registrar_salario(&conn, &salario).is_err() {
                    estado.avisos.push(
                        "não consegui registrar o salário do contracheque no inventário".to_owned(),
                    );
                }
            }
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

        let mut resultado = Resultado {
            sha256: estado.sha256,
            nome,
            ja_existia: estado.ja_existia,
            texto: estado.texto,
            reconhecido: estado.reconhecido,
            sugestoes: estado.sugestoes,
            avisos: estado.avisos,
            entrada: estado.entrada,
            leitura: estado.leitura,
        };
        if let Some(na_caixa) = na_caixa {
            // Duas origens fora a fonte `arrastado` (que o próprio reconhecimento cria ao
            // copiar um arquivo solto): o mesmo conteúdo já foi achado em outro lugar.
            let repetido_por_bytes =
                caixa::origens_de_fora(&conn, &resultado.sha256).is_ok_and(|n| n >= 2);
            // Ou o mesmo texto com bytes diferentes (a Stripe gera o PDF de novo a cada envio;
            // o mesmo recibo chega por e-mail e pelo Telegram): o achado mais antigo segue
            // como está e o mais novo é a possível duplicidade. Quem gravou `texto` por fora
            // (os scripts Bun) deixou a impressão pendente: completa antes de comparar.
            let _ = impressao_do_texto::completar_pendentes(&conn);
            let igual_a = impressao_do_texto::original_do_mesmo_texto(&conn, &resultado.sha256)
                .ok()
                .flatten();
            let repetido = repetido_por_bytes || igual_a.is_some();
            let mut leitura = crate::caixa_de_entrada::leitura_do_resultado(&resultado, repetido);
            if let (false, Some(original), EstadoDaCaixa::PossivelDuplicidade) =
                (repetido_por_bytes, igual_a, leitura.estado)
            {
                let de_quem = format!("mesmo texto de {}", original.nome);
                leitura.resumo = Some(match leitura.resumo.take() {
                    Some(resumo) => format!("{resumo} · {de_quem}"),
                    None => de_quem,
                });
            }
            let gravou = match na_caixa {
                NaCaixa::Registrar(chegada) => caixa::registrar(&conn, &leitura, &chegada).is_ok(),
                NaCaixa::Atualizar => caixa::atualizar(&conn, &leitura).is_ok(),
            };
            if !gravou {
                resultado
                    .avisos
                    .push("não consegui registrar o arquivo na Caixa de entrada".to_owned());
            }
        }
        Ok(resultado)
    }
}

/// Grava a falha da leitura como "Precisa revisar" (fora de "sem padrão de reconhecimento").
fn registrar_falha(conn: &Connection, na_caixa: NaCaixa, visto: &ConteudoVisto) {
    let leitura = caixa::Leitura {
        content_hash: visto.sha256.clone(),
        nome: visto.nome.clone(),
        tipo: None,
        resumo: Some(crate::caixa_de_entrada::RESUMO_LEITURA_FALHOU.to_owned()),
        estado: EstadoDaCaixa::PrecisaRevisar,
    };
    let _ = match na_caixa {
        NaCaixa::Registrar(chegada) => caixa::registrar(conn, &leitura, &chegada).is_ok(),
        NaCaixa::Atualizar => caixa::atualizar(conn, &leitura).is_ok(),
    };
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

/// Sem texto: se a etapa falhou, o texto é "a leitura falhou" (a Caixa de entrada pede
/// revisão); se só não havia o que ler (imagem sem OCR, formato sem leitor), é "sem texto".
fn texto_sem_conteudo(motivo: &str, situacao: &Situacao) -> EstadoDoTexto {
    if matches!(situacao, Situacao::Falhou(_)) {
        EstadoDoTexto::Falhou(motivo.to_owned())
    } else {
        EstadoDoTexto::SemTexto(motivo.to_owned())
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
        if self.so_texto_guardado {
            let motivo = "sem texto guardado: use Reconhecer de novo para extrair".to_owned();
            return Ok((
                EstadoDoTexto::SemTexto(motivo.clone()),
                None,
                Situacao::Pulada(motivo),
            ));
        }
        match extensao(caminho).as_str() {
            "pdf" => self.extrair_do_pdf(conn, caminho, sha),
            // O XML de NF-e é texto: as regras casam pelo conteúdo dele.
            "txt" | "xml" => extrair_do_txt(conn, caminho, sha),
            e if sbf_ocr::EXTENSOES_DE_IMAGEM.contains(&e) => {
                self.extrair_da_imagem(conn, caminho, sha)
            }
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

    /// Imagem (print de app, foto de comprovante): OCR com o tesseract, gravado em `texto`
    /// com o método `ocr-tesseract`. Sem tesseract não é erro e nada é gravado.
    fn extrair_da_imagem(
        &self,
        conn: &Connection,
        caminho: &Path,
        sha: &str,
    ) -> Result<(EstadoDoTexto, Option<String>, Situacao), ErroDoReconhecimento> {
        let sem = |motivo: &str, situacao: Situacao| {
            Ok((texto_sem_conteudo(motivo, &situacao), None, situacao))
        };
        match sbf_ocr::ocr_do_achado(conn, sha, caminho, &self.ocr).map_err(banco)? {
            DesfechoDoOcr::Lido(t) => Ok((EstadoDoTexto::Extraido, Some(t), Situacao::Feita)),
            DesfechoDoOcr::JaHavia(t) => Ok((
                EstadoDoTexto::JaHavia,
                (!t.trim().is_empty()).then_some(t),
                Situacao::Pulada("o inventário já tinha o texto".to_owned()),
            )),
            DesfechoDoOcr::Indisponivel => {
                let m = "OCR indisponível";
                sem(m, Situacao::Pulada(m.to_owned()))
            }
            DesfechoDoOcr::SemTexto => {
                let m = "o OCR não achou texto na imagem";
                sem(m, Situacao::Pulada(m.to_owned()))
            }
            DesfechoDoOcr::Falhou(FalhaDoOcr::ImagemGrande) => {
                let m = "a imagem passa do limite do OCR (15 MiB)";
                sem(m, Situacao::Pulada(m.to_owned()))
            }
            DesfechoDoOcr::Falhou(_) => {
                let m = "o OCR não leu a imagem";
                sem(m, Situacao::Falhou(m.to_owned()))
            }
            _ => {
                let m = "o OCR não deu resultado";
                sem(m, Situacao::Falhou(m.to_owned()))
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
            Ok((texto_sem_conteudo(motivo, &situacao), None, situacao))
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

    fn ler_localmente(&self, conn: &Connection, estado: &mut Estado, caminho: &Path) -> Situacao {
        // O motor de regras vai primeiro; sem regra que se aplique, segue o caminho de sempre.
        if let Some(situacao) = regras::ler_com_regras(self, conn, estado, caminho) {
            return situacao;
        }
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
        ler_contracheque(estado)
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
        let do_contracheque = self.sugerir_do_contracheque(estado);
        let Some(rec) = estado.regra.clone().filter(|r| !r.propostas.is_empty()) else {
            return do_contracheque;
        };
        let das_regras = regras::sugerir_das_propostas(
            &self.cofre,
            &rec,
            &estado.sha256,
            estado.entrada.as_ref(),
            &mut estado.avisos,
        );
        match (das_regras, do_contracheque) {
            (Ok(novas), anterior) => {
                let tinha = !novas.is_empty();
                estado.sugestoes.extend(novas);
                if tinha && matches!(anterior, Situacao::Pulada(_)) {
                    Situacao::Feita
                } else {
                    anterior
                }
            }
            (Err(m), Situacao::Pulada(_)) => Situacao::Falhou(m),
            (Err(_), anterior) => anterior,
        }
    }

    fn sugerir_do_contracheque(&self, estado: &mut Estado) -> Situacao {
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
            Ok(b) => PlanejamentoSqlite::novo(b, Box::new(SemSaldos))
                .com_movimentos(Box::new(MovimentosDaPluggy::do_cofre(&self.cofre))),
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
        let uso_dos_descontos = uso.clone();
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
                regra: None,
                chave: None,
            });
        }
        if !vigentes.iter().all(|v| *v) {
            sugestoes.push(Sugestao::UsarRubricas {
                rotulo: uso.rotulo(&marcacao),
                uso: Box::new(uso),
            });
        }
        // Os descontos em folha (spec 12): os compromissos que faltam e a baixa dos que já
        // estão ligados às rubricas.
        match propor_descontos(&plan, &uso_dos_descontos) {
            Ok(Some(s)) => sugestoes.push(s),
            Ok(None) => {}
            Err(_) => return Situacao::Falhou("não deu para ler o planejamento".to_owned()),
        }
        // O crédito não se adivinha pela data impressa no contracheque: se o extrato tem o
        // crédito, sugere-se conciliar com ele; senão, nada.
        match sugerir_conciliacao(&plan, &recorrente, c.pagamento, c.liquido) {
            Ok(Some(s)) => sugestoes.push(s),
            Ok(None) => {}
            Err(_) => return Situacao::Falhou("não deu para ler o planejamento".to_owned()),
        }
        match sugestao_de_baixa(&plan, c.pagamento, &estado.sha256) {
            Ok(Some(s)) => sugestoes.push(s),
            Ok(None) => {}
            Err(_) => return Situacao::Falhou("não deu para ler o planejamento".to_owned()),
        }
        estado.sugestoes = sugestoes;
        Situacao::Feita
    }
}

/// A sugestão de conciliar o recorrente do salário com o crédito do extrato, se há um só
/// candidato com pelo menos duas das três provas (documento, valor exato do líquido, conta
/// ou assinatura). Empate no topo não sugere nada; ocorrência já conciliada também não.
fn sugerir_conciliacao(
    plan: &PlanejamentoSqlite,
    recorrente: &Recorrente,
    competencia: Competencia,
    liquido: Option<Centavos>,
) -> Result<Option<Sugestao>, sbf_planejamento::ErroDoPlanejamento> {
    let hoje = Data::hoje_em_sao_paulo();
    let Some(ocorrencia) = plan
        .proximas(&recorrente.id, competencia, 1, hoje)?
        .into_iter()
        .find(|o| o.id.competencia == competencia && o.conciliado.is_none())
    else {
        return Ok(None);
    };
    let candidatos = plan.candidatos_de_conciliacao(&ocorrencia.id)?;
    // 1 (o documento é o contracheque deste mês) + o valor exato + a conta ou a assinatura.
    let pontuados: Vec<(u8, &sbf_planejamento::MovimentoCandidato)> = candidatos
        .iter()
        .filter(|c| c.movimento.valor.0 > 0)
        .map(|c| {
            let exato = liquido.is_some_and(|l| c.movimento.valor == l);
            let conta = c.mesma_conta || c.bate_assinatura;
            (1 + u8::from(exato) + u8::from(conta), c)
        })
        .filter(|(provas, _)| *provas >= 2)
        .collect();
    let Some(melhor) = pontuados.iter().map(|(p, _)| *p).max() else {
        return Ok(None);
    };
    let mut do_topo = pontuados.iter().filter(|(p, _)| *p == melhor);
    let (Some((provas, c)), None) = (do_topo.next(), do_topo.next()) else {
        return Ok(None);
    };
    let m = &c.movimento;
    Ok(Some(Sugestao::Conciliar {
        rotulo: format!(
            "Conciliar o {} · {:02}/{} com o crédito de {:02}/{:02} no extrato",
            recorrente.nome, competencia.mes, competencia.ano, m.data.dia, m.data.mes
        ),
        conciliacao: Conciliacao {
            ocorrencia: ocorrencia.id,
            movimento: m.movimento.clone(),
            valor: Centavos(m.valor.0.abs()),
            data: m.data,
            confirmada: false,
        },
        provas: *provas,
    }))
}

/// Lê o texto como contracheque (o leitor nativo) e põe o resultado no estado. Assume que há
/// texto e que ele parece um demonstrativo de pagamento.
pub(crate) fn ler_contracheque(estado: &mut Estado) -> Situacao {
    let Some(conteudo) = estado.conteudo.as_deref() else {
        return Situacao::Pulada("sem texto para os leitores locais".to_owned());
    };
    match sbf_planilha::contracheque::ler(conteudo) {
        Ok(c) => {
            if !c.conferido {
                estado
                    .avisos
                    .push("a soma das rubricas não bate com o líquido do contracheque".to_owned());
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

pub(crate) fn rotulo_do_tipo(tipo: &str) -> &'static str {
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
