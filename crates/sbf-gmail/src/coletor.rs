//! A coleta do Gmail para o cofre.
//!
//! Para cada mensagem que casa com a consulta (da mais velha para a mais nova, para o
//! checkpoint só andar para frente):
//!
//! 1. cada **anexo** aceito (PDF, imagem, XML, planilha, OFX, CSV; imagens embutidas na
//!    assinatura ficam de fora) é baixado para `<cofre>/entrada/gmail/`, vira achado com
//!    origem `gmail:<mensagem>:<parte>` e, **se o conteúdo é novo**, passa pelo
//!    reconhecimento com as regras; o mesmo PDF em dois e-mails é um documento com duas
//!    origens e é reconhecido uma vez;
//! 2. uma mensagem **sem anexo aceito** vira uma entrada do tipo Mensagem: o texto (assunto e
//!    corpo) é guardado como `.txt` (origem `gmail:<mensagem>:corpo`) e passa pelas regras.
//!
//! Origem já conhecida não baixa nada de novo. O checkpoint é o `internalDate` da última
//! mensagem processada antes da primeira falha, guardado em `varredura.cursor`; uma falha
//! numa mensagem não impede as outras, mas congela o checkpoint nela, para a próxima coleta
//! tentar de novo.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use sbf_coletor::gmail::consulta::{PartesDaConsulta, data_do_gmail, montar_consulta};
use sbf_coletor::gmail::mensagem::{MensagemDoGmail, Parte, nome_seguro, processar};
use sbf_reconhecimento::regras::MensagemRecebida;
use sbf_regras::Canal;

use crate::corpo::texto_do_corpo;
use crate::erro::{Erro, Resultado};
use crate::fonte::FonteDeGmail;
use crate::gravador::{Arquivo, DaMensagem, Gravador};
use crate::reconhecimento::{Desfecho, Reconhecimento};

/// A consulta de quando o usuário não diz nada.
pub const CONSULTA_PADRAO: &str = "has:attachment newer_than:30d -category:promotions";
/// Quantas mensagens uma coleta processa, no máximo (o resto fica para a próxima).
pub const LIMITE_PADRAO: usize = 200;
/// Quantas páginas de 100 ids a listagem percorre.
const PAGINAS_MAXIMAS: usize = 30;
/// Folga do checkpoint: relê a última hora (a origem já vista é pulada sem baixar nada).
const FOLGA_DO_CHECKPOINT_S: i64 = 3600;

/// O que o usuário pode ajustar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpcoesDaColeta {
    /// A consulta do Gmail; `None` usa [`CONSULTA_PADRAO`].
    pub consulta: Option<String>,
    /// `AAAA-MM-DD`: coleta desde esta data e ignora o checkpoint.
    pub desde: Option<String>,
    /// Mensagens por coleta; `None` usa [`LIMITE_PADRAO`].
    pub limite: Option<usize>,
}

/// O que uma coleta fez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumoDaColeta {
    /// A consulta final enviada ao Gmail.
    pub consulta: String,
    /// Mensagens lidas.
    pub mensagens: usize,
    /// Anexos com origem nova.
    pub anexos_novos: usize,
    /// Origens que o inventário já tinha (nada foi baixado de novo).
    pub ja_conhecidos: usize,
    /// Conteúdos novos no inventário (anexos e textos).
    pub documentos_novos: usize,
    /// Origem nova de um conteúdo que o inventário já tinha (o mesmo PDF em outro e-mail).
    pub duplicados: usize,
    /// E-mails sem anexo cujo texto virou entrada Mensagem.
    pub mensagens_de_texto: usize,
    /// Quantos o reconhecimento reconheceu.
    pub reconhecidos: usize,
    /// Quantos ficaram ambíguos (precisam de revisão).
    pub ambiguos: usize,
    /// Quantos nenhuma regra reconheceu.
    pub sem_regra: usize,
    /// Mensagens que falharam (a próxima coleta tenta de novo).
    pub falhas: usize,
    /// Mensagens que casam com a consulta e ficaram para a próxima coleta (passou do limite).
    pub restantes: usize,
}

impl ResumoDaColeta {
    /// Algo entrou no inventário?
    #[must_use]
    pub fn novos(&self) -> usize {
        self.documentos_novos + self.duplicados
    }
}

fn iso_de_ms(ms: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// A consulta final: a do usuário (ou a padrão) mais o recorte de tempo.
fn consulta_final(opcoes: &OpcoesDaColeta, cursor_ms: Option<i64>) -> Resultado<String> {
    let base = opcoes
        .consulta
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or(CONSULTA_PADRAO);
    if let Some(desde) = opcoes.desde.as_deref() {
        if data_do_gmail(Some(desde)).is_none() {
            return Err(Erro::Argumento(
                "a data de --desde precisa ser AAAA-MM-DD".to_owned(),
            ));
        }
        return Ok(montar_consulta(&PartesDaConsulta {
            consulta: Some(base),
            de: Some(desde),
            ate: None,
        }));
    }
    Ok(match cursor_ms {
        Some(ms) => {
            let desde = (ms / 1000 - FOLGA_DO_CHECKPOINT_S).max(0);
            format!("{base} after:{desde}")
        }
        None => base.to_owned(),
    })
}

/// A parte `id` é uma imagem embutida no corpo (assinatura, logotipo)?
fn imagem_embutida(partes: &[Parte], id: &str) -> bool {
    for p in partes {
        if p.part_id == id {
            let embutida = p.headers.iter().any(|h| {
                h.name.eq_ignore_ascii_case("content-id")
                    || (h.name.eq_ignore_ascii_case("content-disposition")
                        && h.value.trim_start().to_lowercase().starts_with("inline"))
            });
            return embutida && p.mime_type.to_lowercase().starts_with("image/");
        }
        if imagem_embutida(&p.parts, id) {
            return true;
        }
    }
    false
}

/// O nome do `.txt` do corpo: o assunto, seguro para arquivo.
fn nome_do_corpo(assunto: &str) -> String {
    let base: String = nome_seguro(assunto).chars().take(60).collect();
    let base = base.trim_matches(['_', '.']).to_owned();
    if base.is_empty() {
        "mensagem.txt".to_owned()
    } else {
        format!("{base}.txt")
    }
}

struct Corrida<'a> {
    gravador: &'a Gravador,
    reconhecimento: &'a dyn Reconhecimento,
    varredura: i64,
    resumo: ResumoDaColeta,
}

impl Corrida<'_> {
    fn contar(&mut self, d: &Desfecho) {
        match d {
            Desfecho::Reconhecido(_) => self.resumo.reconhecidos += 1,
            Desfecho::Ambiguo => self.resumo.ambiguos += 1,
            Desfecho::Nada => self.resumo.sem_regra += 1,
        }
    }

    /// Processa uma mensagem. `true`: trouxe alguma origem nova (conta para o limite).
    fn mensagem(&mut self, fonte: &dyn FonteDeGmail, m: &MensagemDoGmail) -> Resultado<bool> {
        let p = processar(m);
        let ms = m.internal_date.parse::<i64>().ok();
        let data = ms
            .and_then(iso_de_ms)
            .or_else(|| {
                sbf_coletor::gmail::mensagem::data_do_cabecalho(&p.metadados.data)
                    .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            })
            .unwrap_or_default();
        let da = DaMensagem {
            id: m.id.clone(),
            conversa: m.thread_id.clone(),
            de: p.metadados.de.clone(),
            assunto: p.metadados.assunto.clone(),
            data,
        };
        let anexos: Vec<_> = p
            .anexos
            .iter()
            .filter(|a| !imagem_embutida(&m.payload.parts, &a.id_da_parte))
            .collect();
        if anexos.is_empty() {
            return self.corpo(m, &da);
        }
        let mut erro = None;
        let mut novo = false;
        for a in anexos {
            let parte = if a.id_da_parte.is_empty() {
                a.id_do_anexo.as_str()
            } else {
                a.id_da_parte.as_str()
            };
            let chave = format!("gmail:{}:{parte}", m.id);
            if self.gravador.origem_existe(&chave)? {
                self.resumo.ja_conhecidos += 1;
                continue;
            }
            let bytes = fonte.anexo(&m.id, &a.id_do_anexo)?;
            let g = self.gravador.gravar(
                self.varredura,
                &da,
                &Arquivo {
                    chave: &chave,
                    nome: &a.nome,
                    mime: &a.tipo_mime,
                    bytes: &bytes,
                    papel: "anexo",
                    parte: Some(parte),
                },
            )?;
            novo = true;
            self.resumo.anexos_novos += 1;
            if !g.achado_novo {
                self.resumo.duplicados += 1;
                continue;
            }
            self.resumo.documentos_novos += 1;
            match self.reconhecimento.arquivo(&g.caminho, &m.id) {
                Ok(d) => self.contar(&d),
                Err(e) => erro = Some(Erro::Reconhecimento(e)),
            }
        }
        erro.map_or(Ok(novo), Err)
    }

    fn corpo(&mut self, m: &MensagemDoGmail, da: &DaMensagem) -> Resultado<bool> {
        let chave = format!("gmail:{}:corpo", m.id);
        if self.gravador.origem_existe(&chave)? {
            self.resumo.ja_conhecidos += 1;
            return Ok(false);
        }
        let corpo = texto_do_corpo(m);
        if corpo.is_empty() {
            return Ok(false);
        }
        let texto = if da.assunto.trim().is_empty() {
            corpo
        } else {
            format!("{}\n\n{corpo}", da.assunto.trim())
        };
        let g = self.gravador.gravar(
            self.varredura,
            da,
            &Arquivo {
                chave: &chave,
                nome: &nome_do_corpo(&da.assunto),
                mime: "text/plain",
                bytes: texto.as_bytes(),
                papel: "eml",
                parte: None,
            },
        )?;
        self.resumo.mensagens_de_texto += 1;
        if !g.achado_novo {
            self.resumo.duplicados += 1;
            return Ok(true);
        }
        self.resumo.documentos_novos += 1;
        let id = format!("gmail:{}", m.id);
        let r = self
            .reconhecimento
            .mensagem(&MensagemRecebida {
                canal: Canal::Gmail,
                id: &id,
                id_externo: &m.id,
                texto: &texto,
                momento: (!da.data.is_empty()).then_some(da.data.as_str()),
                data: da.data.get(..10),
            })
            .map_err(Erro::Reconhecimento)?;
        self.contar(&r);
        Ok(true)
    }
}

/// Roda uma coleta.
///
/// # Errors
/// [`Erro::Argumento`] (data inválida), [`Erro::Banco`] (inventário), [`Erro::Gmail`] (a
/// listagem falhou), [`Erro::Cancelado`]. Falha numa mensagem **não** é erro da coleta: vai
/// para `falhas` do resumo e a próxima coleta tenta de novo (uma falha do Gmail durante a
/// leitura interrompe a corrida e é devolvida).
pub fn coletar(
    cofre: &Path,
    fonte: &dyn FonteDeGmail,
    reconhecimento: &dyn Reconhecimento,
    opcoes: &OpcoesDaColeta,
    cancelar: &AtomicBool,
) -> Resultado<ResumoDaColeta> {
    let gravador = Gravador::abrir(cofre)?;
    let cursor = gravador.cursor()?;
    let consulta = consulta_final(opcoes, cursor)?;
    let varredura = gravador.abrir_varredura(&consulta)?;
    let limite = opcoes.limite.unwrap_or(LIMITE_PADRAO).max(1);

    let fechar = |c: &Corrida<'_>, status: &str, cursor: Option<i64>| {
        let _ = gravador.fechar_varredura(
            varredura,
            status,
            cursor,
            c.resumo.mensagens,
            c.resumo.documentos_novos,
            c.resumo.ja_conhecidos + c.resumo.duplicados,
            c.resumo.falhas,
            &format!(
                "{{\"anexos_novos\":{},\"mensagens_de_texto\":{},\"reconhecidos\":{},\"ambiguos\":{},\"sem_regra\":{}}}",
                c.resumo.anexos_novos,
                c.resumo.mensagens_de_texto,
                c.resumo.reconhecidos,
                c.resumo.ambiguos,
                c.resumo.sem_regra
            ),
        );
    };
    let mut corrida = Corrida {
        gravador: &gravador,
        reconhecimento,
        varredura,
        resumo: ResumoDaColeta {
            consulta: consulta.clone(),
            ..ResumoDaColeta::default()
        },
    };

    // Todos os ids primeiro (barato), depois do mais velho para o mais novo.
    let mut ids: Vec<String> = Vec::new();
    let mut pagina: Option<String> = None;
    for _ in 0..PAGINAS_MAXIMAS {
        if cancelar.load(Ordering::Relaxed) {
            fechar(&corrida, "interrompida", None);
            return Err(Erro::Cancelado);
        }
        let p = match fonte.listar(&consulta, pagina.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                fechar(&corrida, "falhou", None);
                return Err(e);
            }
        };
        ids.extend(p.messages.into_iter().map(|r| r.id));
        pagina = p.next_page_token;
        if pagina.is_none() {
            break;
        }
    }
    ids.reverse();

    // O limite conta as mensagens que trouxeram algo novo: as que o checkpoint relê de folga
    // (origens já vistas) não gastam o limite.
    let mut com_novidade = 0_usize;
    let mut novo_cursor: Option<i64> = None;
    let mut travado = false;
    for (indice, id) in ids.iter().enumerate() {
        if com_novidade >= limite {
            corrida.resumo.restantes = ids.len() - indice;
            break;
        }
        if cancelar.load(Ordering::Relaxed) {
            fechar(&corrida, "interrompida", novo_cursor);
            return Err(Erro::Cancelado);
        }
        let m = match fonte.mensagem(id) {
            Ok(m) => m,
            Err(e) => {
                corrida.resumo.falhas += 1;
                fechar(&corrida, "falhou", novo_cursor);
                return Err(e);
            }
        };
        corrida.resumo.mensagens += 1;
        match corrida.mensagem(fonte, &m) {
            Ok(novo) => com_novidade += usize::from(novo),
            Err(Erro::Gmail(e)) => {
                corrida.resumo.falhas += 1;
                fechar(&corrida, "falhou", novo_cursor);
                return Err(Erro::Gmail(e));
            }
            Err(_) => {
                corrida.resumo.falhas += 1;
                com_novidade += 1;
                travado = true;
            }
        }
        if !travado && let Ok(ms) = m.internal_date.parse::<i64>() {
            novo_cursor = Some(novo_cursor.map_or(ms, |c| c.max(ms)));
        }
    }
    fechar(&corrida, "concluida", novo_cursor);
    Ok(corrida.resumo)
}
