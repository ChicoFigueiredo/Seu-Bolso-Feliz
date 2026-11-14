//! Roda os lotes da triagem (spec 09, "Como gasta pouco", itens 2 e 3).
//!
//! - **Lotes** de até 25 itens por chamada, com resposta validada item a item.
//! - **Retentativa (IA-03):** o item cuja resposta não valeu volta **uma vez**, num lote
//!   só dos inválidos; se falhar de novo, vira `nao_classificado` (uma sugestão `recusada`
//!   com esse campo, que também o tira da fila desta versão do prompt).
//! - **Dois degraus:** a primeira passada é no degrau [`Degrau::Rapido`]; o item cuja
//!   confiança do campo-chave ficou abaixo de [`CONFIANCA_PARA_SUBIR`] sobe, num lote
//!   próprio, para o [`Degrau::Forte`]. Nunca há terceiro degrau.
//! - **Pausa e retomada:** [`Controle::pedir_pausa`] vale entre dois lotes. Retomar é
//!   rodar de novo sobre a fila recalculada: o que já tem sugestão não volta (IA-04).
//! - **Limite e teto:** o ritmo (esperar e continuar) é do motor, que embrulha o
//!   `Provedor`. Se um `ErroDeIa::Limite` ou `Teto` chegar até aqui, a rodada fica
//!   `pausada`, com o que já foi gravado.
//!
//! O cache e o registro do que saiu (`saida_ia`) são do motor: esta rodada só usa o
//! `Provedor` que receber e nunca guarda o texto enviado.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use sbf_ia::{Consumo, Degrau, ErroDeIa, Pedido, Provedor, Resposta};

use crate::entrada::{Vocabulario, montar_lote, remontar};
use crate::erro::Resultado;
use crate::normalizar::normalizar;
use crate::prompt::{
    CONFIANCA_PARA_SUBIR, INSTRUCAO, ITENS_POR_LOTE, PROMPT_VERSAO, esquema, max_tokens_de_saida,
};
use crate::resposta::{SugestaoValida, confianca_chave, interpretar};
use crate::sugestoes::{EstadoDaSugestao, NovaSugestao, atualizar_rodada, gravar, iniciar_rodada};
use crate::tipos::{Alvo, Item};

/// Falhas seguidas do provedor que encerram a rodada como `falhou`.
pub const FALHAS_SEGUIDAS_PARA_ABORTAR: u32 = 3;

#[derive(Debug, Clone)]
pub struct ConfigDaRodada {
    pub itens_por_lote: usize,
    pub max_caracteres_do_lote: usize,
    pub confianca_para_subir: f64,
    pub prompt_versao: String,
    /// O piloto (IA-09): não mostra à IA o que o classificador já pôs, não descarta
    /// sugestão igual ao valor atual e grava tudo como `obsoleta`.
    pub piloto: bool,
}

impl Default for ConfigDaRodada {
    fn default() -> Self {
        Self {
            itens_por_lote: ITENS_POR_LOTE,
            max_caracteres_do_lote: crate::entrada::LIMITE_DO_LOTE,
            confianca_para_subir: CONFIANCA_PARA_SUBIR,
            prompt_versao: PROMPT_VERSAO.to_owned(),
            piloto: false,
        }
    }
}

/// O pedido de pausa, compartilhado com a thread da rodada.
#[derive(Debug, Default)]
pub struct Controle {
    pausa: AtomicBool,
}

impl Controle {
    pub fn novo() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn pedir_pausa(&self) {
        self.pausa.store(true, Ordering::SeqCst);
    }

    pub fn limpar(&self) {
        self.pausa.store(false, Ordering::SeqCst);
    }

    pub fn pausa_pedida(&self) -> bool {
        self.pausa.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituacaoDaRodada {
    Ociosa,
    Rodando,
    Pausada,
    Concluida,
    Falhou,
}

impl SituacaoDaRodada {
    /// O texto de `rodada_ia.situacao`.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::Ociosa | Self::Rodando => "rodando",
            Self::Pausada => "pausada",
            Self::Concluida => "concluida",
            Self::Falhou => "falhou",
        }
    }
}

/// O andamento da rodada, para a barra da janela.
#[derive(Debug, Clone, PartialEq)]
pub struct Progresso {
    pub situacao: SituacaoDaRodada,
    pub rodada_id: Option<i64>,
    pub total: usize,
    pub processados: usize,
    pub chamadas: u32,
    pub sugestoes: usize,
    pub nao_classificados: usize,
    pub subiram_de_degrau: usize,
    pub tokens_entrada: u64,
    pub tokens_saida: u64,
    /// Nominal na assinatura. Dezmilésimos de dólar.
    pub custo_dezmilesimos_usd: u64,
    /// Uma frase para a tela: por que pausou ou falhou.
    pub aviso: Option<String>,
}

impl Progresso {
    pub fn ocioso() -> Self {
        Self {
            situacao: SituacaoDaRodada::Ociosa,
            rodada_id: None,
            total: 0,
            processados: 0,
            chamadas: 0,
            sugestoes: 0,
            nao_classificados: 0,
            subiram_de_degrau: 0,
            tokens_entrada: 0,
            tokens_saida: 0,
            custo_dezmilesimos_usd: 0,
            aviso: None,
        }
    }

    pub fn fracao(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.processados as f32 / self.total as f32).min(1.0)
        }
    }

    /// O custo nominal em centavos de dólar (arredondado para baixo).
    pub fn custo_centavos_usd(&self) -> u64 {
        self.custo_dezmilesimos_usd / 100
    }
}

fn somar(p: &mut Progresso, c: &Consumo) {
    p.tokens_entrada += c.tokens_entrada;
    p.tokens_saida += c.tokens_saida;
    p.custo_dezmilesimos_usd += c.custo_dezmilesimos_usd;
}

type Resultados = Vec<Option<Vec<SugestaoValida>>>;

/// Uma chamada: monta o pedido, chama, valida. Uma resposta fora do esquema conta como
/// "nenhum item valeu" (a retentativa cuida), não como erro da rodada.
fn chamar(
    provedor: &dyn Provedor,
    vocabulario: &Vocabulario,
    itens: &[Item],
    degrau: Degrau,
    versao: &str,
    progresso: &Mutex<Progresso>,
) -> Result<(Resultados, String), ErroDeIa> {
    let pedido = Pedido {
        prompt_versao: versao.to_owned(),
        instrucao: INSTRUCAO.to_owned(),
        entrada: remontar(vocabulario, itens),
        esquema: esquema(),
        degrau,
        max_tokens_saida: max_tokens_de_saida(itens.len()),
    };
    let resultado = provedor.pedir(&pedido);
    if let Ok(mut p) = progresso.lock() {
        p.chamadas += 1;
    }
    match resultado {
        Ok(Resposta {
            json,
            consumo,
            modelo,
        }) => {
            if let Ok(mut p) = progresso.lock() {
                somar(&mut p, &consumo);
            }
            Ok((interpretar(&json, itens), modelo))
        }
        Err(ErroDeIa::RespostaInvalida(_)) => Ok((vec![None; itens.len()], String::new())),
        Err(outro) => Err(outro),
    }
}

/// O resultado final de um item do lote.
#[derive(Debug, Clone)]
enum Destino {
    Sugestoes(Vec<SugestaoValida>, String),
    NaoClassificado,
    /// A chamada falhou antes de dar duas respostas: o item volta numa próxima rodada.
    Adiado,
}

/// Processa um lote: primeira passada, retentativa dos inválidos e subida de degrau. O
/// erro (se houve) vem junto com o que já deu certo, para não se perder.
fn processar_lote(
    provedor: &dyn Provedor,
    vocabulario: &Vocabulario,
    itens: &[Item],
    config: &ConfigDaRodada,
    progresso: &Mutex<Progresso>,
) -> (Vec<Destino>, Option<ErroDeIa>) {
    let mut destinos: Vec<Destino> = vec![Destino::Adiado; itens.len()];
    let versao = config.prompt_versao.as_str();

    let (primeira, modelo_rapido) = match chamar(
        provedor,
        vocabulario,
        itens,
        Degrau::Rapido,
        versao,
        progresso,
    ) {
        Ok(r) => r,
        Err(e) => return (destinos, Some(e)),
    };
    let mut invalidos: Vec<usize> = Vec::new();
    for (i, r) in primeira.into_iter().enumerate() {
        match r {
            Some(s) => destinos[i] = Destino::Sugestoes(s, modelo_rapido.clone()),
            None => invalidos.push(i),
        }
    }

    // IA-03: os inválidos voltam uma vez; na segunda, "não classificado".
    let mut erro = None;
    if !invalidos.is_empty() {
        let reenvio: Vec<Item> = invalidos.iter().map(|i| itens[*i].clone()).collect();
        match chamar(
            provedor,
            vocabulario,
            &reenvio,
            Degrau::Rapido,
            versao,
            progresso,
        ) {
            Ok((segunda, modelo)) => {
                for (pos, r) in invalidos.iter().zip(segunda) {
                    destinos[*pos] = match r {
                        Some(s) => Destino::Sugestoes(s, modelo.clone()),
                        None => Destino::NaoClassificado,
                    };
                }
            }
            Err(e) => erro = Some(e),
        }
    }

    // Dois degraus: o que ficou incerto sobe para o forte, num lote só.
    if erro.is_none() {
        let incertos: Vec<usize> = destinos
            .iter()
            .enumerate()
            .filter_map(|(i, d)| match d {
                Destino::Sugestoes(s, _)
                    if confianca_chave(&itens[i], s)
                        .is_some_and(|c| c < config.confianca_para_subir) =>
                {
                    Some(i)
                }
                _ => None,
            })
            .collect();
        if !incertos.is_empty() {
            let reenvio: Vec<Item> = incertos.iter().map(|i| itens[*i].clone()).collect();
            match chamar(
                provedor,
                vocabulario,
                &reenvio,
                Degrau::Forte,
                versao,
                progresso,
            ) {
                Ok((forte, modelo)) => {
                    for (pos, r) in incertos.iter().zip(forte) {
                        if let Some(s) = r {
                            destinos[*pos] = Destino::Sugestoes(s, modelo.clone());
                            if let Ok(mut p) = progresso.lock() {
                                p.subiram_de_degrau += 1;
                            }
                        }
                    }
                }
                Err(e) => erro = Some(e),
            }
        }
    }
    (destinos, erro)
}

/// A sugestão já diz o que o item tem hoje? Então não vale mostrar.
fn igual_ao_atual(item: &Item, s: &SugestaoValida) -> bool {
    let igual = |atual: &Option<String>| {
        atual
            .as_deref()
            .zip(s.valor.as_str())
            .is_some_and(|(a, v)| normalizar(a) == normalizar(v))
    };
    match (item, s.campo.as_str()) {
        (Item::Achado(a), "tipo_documento") => igual(&a.document_type),
        (Item::Achado(a), "fornecedor") => igual(&a.fornecedor),
        (Item::Achado(a), "data_chave") => igual(&a.data_chave),
        (Item::Achado(a), "valor") => {
            a.valor_centavos.is_some() && a.valor_centavos == s.valor.as_i64()
        }
        (Item::Registro(r), "categoria") => igual(&r.categoria),
        (Item::Registro(r), "fornecedor") => igual(&r.fornecedor),
        (Item::Registro(r), "conta") => igual(&r.conta),
        (Item::Registro(r), "tipo") => {
            normalizar(&r.tipo) == s.valor.as_str().map_or_else(String::new, normalizar)
        }
        _ => false,
    }
}

fn gravar_lote(
    conn: &Connection,
    itens: &[Item],
    destinos: &[Destino],
    config: &ConfigDaRodada,
    rodada_id: i64,
    progresso: &Mutex<Progresso>,
) -> Resultado<()> {
    let estado = if config.piloto {
        EstadoDaSugestao::Obsoleta
    } else {
        EstadoDaSugestao::Aberta
    };
    let tx = conn.unchecked_transaction()?;
    let (mut sugestoes, mut nao_classificados, mut processados) = (0, 0, 0);
    for (item, destino) in itens.iter().zip(destinos) {
        let alvo: Alvo = item.alvo();
        match destino {
            Destino::Sugestoes(lista, modelo) => {
                processados += 1;
                for s in lista {
                    if !config.piloto && igual_ao_atual(item, s) {
                        continue;
                    }
                    gravar(
                        &tx,
                        &NovaSugestao::da_ia(
                            &alvo,
                            s,
                            modelo,
                            &config.prompt_versao,
                            rodada_id,
                            estado,
                        ),
                    )?;
                    sugestoes += 1;
                }
            }
            Destino::NaoClassificado => {
                processados += 1;
                nao_classificados += 1;
                gravar(
                    &tx,
                    &NovaSugestao::nao_classificado(&alvo, &config.prompt_versao, rodada_id),
                )?;
            }
            Destino::Adiado => {}
        }
    }
    tx.commit()?;
    if let Ok(mut p) = progresso.lock() {
        p.processados += processados;
        p.sugestoes += sugestoes;
        p.nao_classificados += nao_classificados;
    }
    Ok(())
}

/// O resumo que a rodada devolve (o mesmo [`Progresso`] final).
pub type ResumoDaRodada = Progresso;

fn copiar(progresso: &Mutex<Progresso>) -> Progresso {
    progresso
        .lock()
        .map_or_else(|e| e.into_inner().clone(), |p| p.clone())
}

fn gravar_andamento(
    conn: &Connection,
    rodada_id: i64,
    p: &Progresso,
    terminada: bool,
) -> Resultado<()> {
    atualizar_rodada(
        conn,
        rodada_id,
        i64::try_from(p.processados).unwrap_or(i64::MAX),
        i64::from(p.chamadas),
        i64::try_from(p.custo_centavos_usd()).unwrap_or(i64::MAX),
        p.situacao.codigo(),
        terminada,
    )
}

/// Roda a triagem sobre `alvos` (a fila de [`crate::fila::selecionar`]).
///
/// `progresso` é atualizado a cada lote para a janela ler de outra thread.
///
/// # Errors
/// Falha do SQLite. Falha do provedor **não** é erro: vira a situação `Falhou` ou
/// `Pausada` no resumo.
pub fn rodar(
    conn: &Connection,
    provedor: &dyn Provedor,
    alvos: &[Alvo],
    config: &ConfigDaRodada,
    controle: &Controle,
    progresso: &Mutex<Progresso>,
) -> Resultado<ResumoDaRodada> {
    let rodada_id = iniciar_rodada(conn, provedor.nome(), config.piloto)?;
    if let Ok(mut p) = progresso.lock() {
        *p = Progresso {
            situacao: SituacaoDaRodada::Rodando,
            rodada_id: Some(rodada_id),
            total: alvos.len(),
            ..Progresso::ocioso()
        };
    }
    let vocabulario = Vocabulario::do_banco(conn)?;
    let mut posicao = 0;
    let mut falhas_seguidas = 0;
    let mut situacao = SituacaoDaRodada::Concluida;
    let mut aviso: Option<String> = None;

    while posicao < alvos.len() {
        if controle.pausa_pedida() {
            situacao = SituacaoDaRodada::Pausada;
            aviso = Some("pausada por você".to_owned());
            break;
        }
        let lote = montar_lote(
            conn,
            &alvos[posicao..],
            !config.piloto,
            &vocabulario,
            config.itens_por_lote,
            config.max_caracteres_do_lote,
        )?;
        posicao += lote.consumidos.max(1);
        if lote.itens.is_empty() {
            continue;
        }
        let (destinos, erro) =
            processar_lote(provedor, &vocabulario, &lote.itens, config, progresso);
        gravar_lote(conn, &lote.itens, &destinos, config, rodada_id, progresso)?;
        let andamento = copiar(progresso);
        gravar_andamento(conn, rodada_id, &andamento, false)?;
        match erro {
            None => falhas_seguidas = 0,
            Some(ErroDeIa::Limite { espera_segundos }) => {
                situacao = SituacaoDaRodada::Pausada;
                aviso = Some(format!("limite do provedor; tentar em {espera_segundos}s"));
                break;
            }
            Some(ErroDeIa::Teto) => {
                situacao = SituacaoDaRodada::Pausada;
                aviso = Some("teto de gasto atingido".to_owned());
                break;
            }
            Some(ErroDeIa::Indisponivel(motivo)) => {
                situacao = SituacaoDaRodada::Falhou;
                aviso = Some(format!("provedor indisponível: {motivo}"));
                break;
            }
            Some(outro) => {
                falhas_seguidas += 1;
                if falhas_seguidas >= FALHAS_SEGUIDAS_PARA_ABORTAR {
                    situacao = SituacaoDaRodada::Falhou;
                    aviso = Some(format!("{outro}"));
                    break;
                }
            }
        }
    }

    if let Ok(mut p) = progresso.lock() {
        p.situacao = situacao;
        p.aviso = aviso;
    }
    let final_ = copiar(progresso);
    gravar_andamento(
        conn,
        rodada_id,
        &final_,
        situacao != SituacaoDaRodada::Pausada,
    )?;
    Ok(final_)
}
