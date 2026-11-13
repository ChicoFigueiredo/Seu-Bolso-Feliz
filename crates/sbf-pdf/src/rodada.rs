//! A rodada sobre o inventário (spec 06, "PDF protegido", itens 1 a 8): para cada alvo,
//! as regras em ordem, uma tentativa por vez (fora de qualquer transação), e o registro numa
//! transação IMMEDIATE curta por documento (`sbf_inventario::protegidos`).
//!
//! - A lista padrão de estratégias só entra com [`OpcoesDaRodada::tentar_estrategias_padrao`]
//!   (desligada por padrão): nada de tentativa cega.
//! - Depois de N falhas de senha **acumuladas** por documento (padrão 6), o achado vira a
//!   pendência "PDF protegido sem regra" e as tentativas param. Sem estratégia para tentar
//!   (nenhuma regra e a opção padrão desligada) não há tentativa nem pendência: o achado só
//!   entra na contagem [`RelatorioDaRodada::sem_regra`] ("N PDFs sem regra" na janela).
//! - [`OpcoesDaRodada::so_os_achados`] restringe a rodada à seleção (a Ação "Abrir protegido"
//!   da aba Achado).
//! - Quando uma estratégia **padrão** vence para um par sem regra, a rodada devolve uma
//!   [`PropostaDeRegra`]; gravar a regra é decisão do usuário
//!   ([`PropostaDeRegra::pedido`], que vira `Acao::GuardarRegraDeSenha`).
//! - O original nunca muda: só é lido.

use crate::abertura::{Desfecho, Ferramentas, PdfProtegido};
use crate::estrategia::Estrategia;
use crate::fonte::{ErroDeFonte, FonteDeSegredos};
use crate::texto::{LIMITE_DO_TEXTO, cortar_como_o_js, impressao_canonica};
use rusqlite::Connection;
use sbf_inventario::comando::{Acao, PedidoDeDecisao};
use sbf_inventario::protegidos::{
    self, AlvoProtegido, Conclusao, LIMITE_PADRAO_DE_FALHAS, Registro, ResultadoDaTentativa,
    TIPO_CIFRA_NAO_SUPORTADA, TIPO_CORROMPIDO, TentativaFeita,
};
use sbf_inventario::{Ref, Tipo};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// As opções da rodada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcoesDaRodada {
    /// "Tentar estratégias padrão": desligada por padrão.
    pub tentar_estrategias_padrao: bool,
    /// Falhas de senha por documento antes da pendência (padrão 6).
    pub limite_de_falhas: u32,
    pub ferramentas: Ferramentas,
    /// Só estes achados (`content_hash`), quando a rodada vem de uma seleção; `None` roda
    /// todos os alvos.
    pub so_os_achados: Option<BTreeSet<String>>,
}

impl Default for OpcoesDaRodada {
    fn default() -> Self {
        Self {
            tentar_estrategias_padrao: false,
            limite_de_falhas: LIMITE_PADRAO_DE_FALHAS,
            ferramentas: Ferramentas::default(),
            so_os_achados: None,
        }
    }
}

/// "Faturas do Banco Alfa usam os 5 últimos dígitos do CPF. Guardar como regra?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropostaDeRegra {
    pub fornecedor: String,
    /// O tipo do achado, ou `*` quando ele não tem tipo.
    pub document_type: String,
    pub estrategia: Estrategia,
    pub frase: String,
}

/// O plural do tipo para a frase ("fatura" → "Faturas"); tipo desconhecido vira "Documentos
/// do tipo X".
fn plural_do_tipo(tipo: &str) -> String {
    let conhecido = match tipo {
        "*" => Some("Documentos"),
        "fatura" => Some("Faturas"),
        "boleto" => Some("Boletos"),
        "extrato" => Some("Extratos"),
        "comprovante" => Some("Comprovantes"),
        "contracheque" => Some("Contracheques"),
        "informe" => Some("Informes"),
        "nfe" => Some("Notas fiscais"),
        "recibo" => Some("Recibos"),
        "contrato" => Some("Contratos"),
        _ => None,
    };
    conhecido
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Documentos do tipo {tipo}"))
}

impl PropostaDeRegra {
    fn nova(fornecedor: &str, document_type: Option<&str>, estrategia: Estrategia) -> Self {
        let document_type = document_type.unwrap_or("*").to_owned();
        let frase = format!(
            "{} do {fornecedor} usam {}. Guardar como regra?",
            plural_do_tipo(&document_type),
            estrategia.descricao()
        );
        Self {
            fornecedor: fornecedor.to_owned(),
            document_type,
            estrategia,
            frase,
        }
    }

    /// O pedido que a janela executa (`Comandos::executar`) **só** quando o usuário confirma.
    pub fn pedido(&self) -> PedidoDeDecisao {
        PedidoDeDecisao {
            acao: Acao::GuardarRegraDeSenha {
                document_type: self.document_type.clone(),
                estrategia: self.estrategia.to_string(),
            },
            alvos: vec![Ref::nova(Tipo::Fornecedor, self.fornecedor.clone())],
            motivo: Some(self.frase.clone()),
        }
    }
}

/// O que aconteceu com um documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Situacao {
    /// Abriu com texto, pela estratégia (ou sem senha, `None`).
    Aberto { estrategia: Option<String> },
    /// Abriu e não tem texto (OCR, numa fatia futura).
    SemTexto { estrategia: Option<String> },
    /// Virou pendência (sem regra, corrompido, cifra não suportada).
    Pendencia { tipo: String, id: i64 },
    /// Nenhuma estratégia abriu ainda; outras rodadas podem tentar.
    NaoAbriu,
    /// Não havia estratégia para tentar (nenhuma regra do fornecedor e a opção "tentar
    /// estratégias padrão" desligada). Nada foi tentado nem gravado.
    SemRegra,
    /// A senha conferiu, mas a extração falhou (tenta de novo na próxima rodada).
    FalhaNaExtracao,
    /// Faltou um segredo no chaveiro para alguma estratégia (trancado, sem CPF, sem a senha
    /// própria). Nada foi tentado com ela.
    FaltouSegredo(ErroDeFonte),
    /// Nem o acervo nem a origem local têm o arquivo.
    SemArquivo,
    /// O arquivo passa de [`crate::abertura::TAMANHO_MAXIMO_DO_PDF`]: nem foi lido nem
    /// tentado (achado A10 da revisão de segurança da fatia 1).
    GrandeDemais,
}

/// O resultado de um documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoDoAlvo {
    pub content_hash: String,
    pub situacao: Situacao,
    pub tentativas: usize,
    pub falhas: u32,
    pub proposta: Option<PropostaDeRegra>,
}

/// O resultado da rodada inteira.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelatorioDaRodada {
    pub resultados: Vec<ResultadoDoAlvo>,
    /// Nomes de estratégia em `regra_senha` que o desktop não conhece (ignorados).
    pub estrategias_desconhecidas: Vec<String>,
}

impl RelatorioDaRodada {
    /// As propostas de regra, para a janela perguntar.
    pub fn propostas(&self) -> impl Iterator<Item = &PropostaDeRegra> {
        self.resultados.iter().filter_map(|r| r.proposta.as_ref())
    }

    /// Quantos PDFs ficaram sem estratégia para tentar ("N PDFs sem regra").
    pub fn sem_regra(&self) -> usize {
        self.resultados
            .iter()
            .filter(|r| r.situacao == Situacao::SemRegra)
            .count()
    }
}

/// O arquivo do alvo: a cópia do acervo, senão a origem local com o mesmo tamanho (como o
/// `localizarArquivo` do `e3-extrair.ts`).
///
/// O `caminho_acervo` só vale relativo e sem `..` (não sai do cofre), e a origem local só vale
/// absoluta (achado A14 da revisão de segurança da fatia 1).
pub fn localizar(raiz_do_cofre: &Path, alvo: &AlvoProtegido) -> Option<PathBuf> {
    if let Some(relativo) = alvo
        .caminho_acervo
        .as_deref()
        .filter(|r| relativo_dentro(r))
    {
        let p = raiz_do_cofre.join(relativo);
        if p.is_file() {
            return Some(p);
        }
    }
    alvo.caminhos_locais.iter().find_map(|(caminho, tamanho)| {
        let p = PathBuf::from(caminho);
        if !p.is_absolute() {
            return None;
        }
        let metadata = std::fs::metadata(&p).ok()?;
        let confere = tamanho.is_none_or(|t| u64::try_from(t).ok() == Some(metadata.len()));
        (metadata.is_file() && confere).then_some(p)
    })
}

/// Relativo, não vazio, e só com componentes normais (sem `..`, `.` nem raiz).
fn relativo_dentro(relativo: &str) -> bool {
    let caminho = Path::new(relativo);
    !relativo.is_empty()
        && caminho.is_relative()
        && caminho
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Roda a abertura nos alvos do inventário (todos, ou só os de
/// [`OpcoesDaRodada::so_os_achados`]).
pub fn abrir_protegidos(
    conexao: &Connection,
    raiz_do_cofre: &Path,
    fonte: &dyn FonteDeSegredos,
    opcoes: &OpcoesDaRodada,
) -> rusqlite::Result<RelatorioDaRodada> {
    let mut relatorio = RelatorioDaRodada::default();
    let escolhido = |alvo: &AlvoProtegido| {
        opcoes
            .so_os_achados
            .as_ref()
            .is_none_or(|so| so.contains(&alvo.content_hash))
    };
    for alvo in protegidos::alvos(conexao, opcoes.limite_de_falhas)?
        .into_iter()
        .filter(escolhido)
    {
        let resultado = abrir_um(conexao, raiz_do_cofre, &alvo, fonte, opcoes, &mut relatorio)?;
        relatorio.resultados.push(resultado);
    }
    Ok(relatorio)
}

fn conclusao_do_texto(texto: &str) -> Conclusao {
    Conclusao::Aberto {
        conteudo: cortar_como_o_js(texto, LIMITE_DO_TEXTO),
        impressao: impressao_canonica(texto),
    }
}

fn pendencia_do_desfecho(desfecho: &Desfecho, alvo: &AlvoProtegido) -> Option<Conclusao> {
    let quem = alvo
        .fornecedor
        .as_deref()
        .unwrap_or("fornecedor desconhecido");
    match desfecho {
        Desfecho::Corrompido => Some(Conclusao::Pendencia {
            tipo: TIPO_CORROMPIDO,
            pergunta: format!(
                "PDF protegido corrompido ({quem}): o arquivo não abre nem é legível. Há outra \
                 cópia deste documento?"
            ),
        }),
        Desfecho::CifraNaoSuportada => Some(Conclusao::Pendencia {
            tipo: TIPO_CIFRA_NAO_SUPORTADA,
            pergunta: format!(
                "PDF protegido com cifra não suportada ({quem}): instale o qpdf para o \
                 desktop abrir este documento."
            ),
        }),
        _ => None,
    }
}

/// Abre um alvo e grava o resultado.
fn abrir_um(
    conexao: &Connection,
    raiz_do_cofre: &Path,
    alvo: &AlvoProtegido,
    fonte: &dyn FonteDeSegredos,
    opcoes: &OpcoesDaRodada,
    relatorio: &mut RelatorioDaRodada,
) -> rusqlite::Result<ResultadoDoAlvo> {
    let mut resultado = ResultadoDoAlvo {
        content_hash: alvo.content_hash.clone(),
        situacao: Situacao::SemArquivo,
        tentativas: 0,
        falhas: alvo.falhas,
        proposta: None,
    };
    let Some(caminho) = localizar(raiz_do_cofre, alvo) else {
        return Ok(resultado);
    };
    let pdf = match PdfProtegido::ler(&caminho) {
        Ok(pdf) => pdf,
        Err(erro) if erro.kind() == std::io::ErrorKind::FileTooLarge => {
            return Ok(ResultadoDoAlvo {
                situacao: Situacao::GrandeDemais,
                ..resultado
            });
        }
        Err(_) => return Ok(resultado),
    };
    let ferramentas = &opcoes.ferramentas;
    let limite = opcoes.limite_de_falhas;

    let gravar =
        |tentativas: &[TentativaFeita], conclusao: &Conclusao| -> rusqlite::Result<Registro> {
            protegidos::registrar(conexao, alvo, tentativas, conclusao, limite)
        };
    let situacao_da_pendencia =
        |registro: &Registro, tipo: &str, senao: Situacao| match registro.pendencia {
            Some(id) => Situacao::Pendencia {
                tipo: tipo.to_owned(),
                id,
            },
            None => senao,
        };

    // Abre sem senha (o Bun marcou, mas a senha vazia abre) ou nem é legível.
    if !pdf.precisa_de_senha() {
        let desfecho = pdf.abrir_sem_senha(ferramentas);
        let (conclusao, situacao) = match &desfecho {
            Desfecho::Aberto(t) => (
                conclusao_do_texto(&t.texto),
                Situacao::Aberto { estrategia: None },
            ),
            Desfecho::SemTexto { .. } => {
                (Conclusao::SemTexto, Situacao::SemTexto { estrategia: None })
            }
            Desfecho::FalhaNaExtracao(_) => (Conclusao::SoTentativas, Situacao::FalhaNaExtracao),
            outro => match pendencia_do_desfecho(outro, alvo) {
                Some(c) => (c, Situacao::NaoAbriu),
                None => (Conclusao::SoTentativas, Situacao::NaoAbriu),
            },
        };
        let tipo = match &conclusao {
            Conclusao::Pendencia { tipo, .. } => *tipo,
            _ => "",
        };
        let registro = gravar(&[], &conclusao)?;
        resultado.situacao = situacao_da_pendencia(&registro, tipo, situacao);
        resultado.falhas = registro.falhas;
        return Ok(resultado);
    }

    // A lista: regras do par, regras do fornecedor com `*`, e (só com a opção) a padrão.
    let mut lista: Vec<(Estrategia, Option<i64>)> = Vec::new();
    for regra in protegidos::regras_para(
        conexao,
        alvo.fornecedor.as_deref(),
        alvo.document_type.as_deref(),
    )? {
        match regra.estrategia.parse::<Estrategia>() {
            Ok(e) if !lista.iter().any(|(x, _)| *x == e) => lista.push((e, Some(regra.id))),
            Ok(_) => {}
            Err(_) => {
                if !relatorio
                    .estrategias_desconhecidas
                    .contains(&regra.estrategia)
                {
                    relatorio
                        .estrategias_desconhecidas
                        .push(regra.estrategia.clone());
                }
            }
        }
    }
    if opcoes.tentar_estrategias_padrao {
        for e in Estrategia::padrao() {
            if !lista.iter().any(|(x, _)| *x == e) {
                lista.push((e, None));
            }
        }
    }

    // Sem estratégia para tentar: nem tentativa nem pendência (spec 06, item 7: pendência só
    // depois de N falhas). A janela mostra a contagem.
    if lista.is_empty() {
        resultado.situacao = Situacao::SemRegra;
        return Ok(resultado);
    }

    let mut tentativas = Vec::new();
    let mut falhas = alvo.falhas;
    let mut conclusao = None;
    let mut situacao = Situacao::NaoAbriu;
    let mut faltou = None;
    for (estrategia, regra) in &lista {
        if falhas >= limite {
            break;
        }
        let candidata = match estrategia.candidata(fonte) {
            Ok(c) => c,
            Err(erro) => {
                faltou.get_or_insert(erro);
                continue;
            }
        };
        let desfecho = pdf.tentar(&candidata, ferramentas);
        drop(candidata);
        let abriu = |tentativas: &mut Vec<TentativaFeita>| {
            tentativas.push(TentativaFeita {
                estrategia: estrategia.to_string(),
                regra: *regra,
                resultado: ResultadoDaTentativa::Abriu,
            });
        };
        match desfecho {
            Desfecho::SenhaErrada => {
                tentativas.push(TentativaFeita {
                    estrategia: estrategia.to_string(),
                    regra: *regra,
                    resultado: ResultadoDaTentativa::SenhaErrada,
                });
                falhas += 1;
                continue;
            }
            Desfecho::Aberto(t) => {
                abriu(&mut tentativas);
                conclusao = Some(conclusao_do_texto(&t.texto));
                situacao = Situacao::Aberto {
                    estrategia: Some(estrategia.to_string()),
                };
            }
            Desfecho::SemTexto { .. } => {
                abriu(&mut tentativas);
                conclusao = Some(Conclusao::SemTexto);
                situacao = Situacao::SemTexto {
                    estrategia: Some(estrategia.to_string()),
                };
            }
            Desfecho::FalhaNaExtracao(_) => {
                abriu(&mut tentativas);
                conclusao = Some(Conclusao::SoTentativas);
                situacao = Situacao::FalhaNaExtracao;
            }
            outro => {
                conclusao = pendencia_do_desfecho(&outro, alvo);
            }
        }
        // Abriu: a proposta, se venceu uma estratégia padrão para um par sem regra.
        if regra.is_none()
            && matches!(
                situacao,
                Situacao::Aberto { .. } | Situacao::SemTexto { .. }
            )
            && let Some(fornecedor) = alvo.fornecedor.as_deref().filter(|f| !f.trim().is_empty())
        {
            resultado.proposta = Some(PropostaDeRegra::nova(
                fornecedor,
                alvo.document_type.as_deref(),
                estrategia.clone(),
            ));
        }
        break;
    }

    // Nada abriu: só as tentativas. A pendência "sem regra" sai do `registrar` quando as
    // falhas acumuladas chegam ao limite; faltar segredo não é falha (a janela pede para
    // destrancar o chaveiro).
    let conclusao = conclusao.unwrap_or(Conclusao::SoTentativas);
    let tipo = match &conclusao {
        Conclusao::Pendencia { tipo, .. } => *tipo,
        _ => protegidos::TIPO_SEM_REGRA,
    };
    let registro = gravar(&tentativas, &conclusao)?;
    if let (Situacao::NaoAbriu, Some(erro)) = (&situacao, faltou) {
        situacao = Situacao::FaltouSegredo(erro);
    }
    resultado.situacao = situacao_da_pendencia(&registro, tipo, situacao);
    resultado.tentativas = tentativas.len();
    resultado.falhas = registro.falhas;
    Ok(resultado)
}
