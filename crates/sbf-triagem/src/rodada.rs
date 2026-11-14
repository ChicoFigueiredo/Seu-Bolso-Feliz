//! Roda a triagem sobre a fila do motor (`sbf_ia::Fila`, spec 09, "Como gasta pouco").
//!
//! O motor cuida de: lotes de até 25 itens, cache por conteúdo (IA-04), retentativa única
//! do item sem resposta válida (IA-03), dois degraus (confiança abaixo de 0,7 sobe para o
//! forte, nunca para um terceiro), espera no limite e reserva só se ligada e abaixo do teto
//! (IA-06), pausa, e o registro do que saiu (`saida_ia`, sem texto, IA-05).
//!
//! Esta rodada faz o que é do domínio: carrega os itens em **pedaços** de
//! [`ITENS_POR_PEDACO`] (para não guardar 10 mil textos na memória), entrega cada pedaço à
//! fila, valida a resposta item a item ([`crate::resposta`]) e grava as sugestões. O item
//! que o motor devolve como `nao_classificado` vira uma sugestão `recusada` com esse campo
//! (some da lista e não volta à fila desta versão do prompt). Retomar é rodar de novo sobre
//! a fila recalculada: o que já tem sugestão não volta.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use sbf_ia::{
    ConfiguracaoDaFila, Controle, Degrau, Evento, Fila, Interrupcao, NivelDeConfianca, Relatorio,
};

use crate::aceite::registro_promovido;
use crate::entrada::{Vocabulario, carregar_itens};
use crate::erro::Resultado;
use crate::formato::FormatoDaTriagem;
use crate::normalizar::normalizar;
use crate::prompt::{INSTRUCAO, ITENS_POR_LOTE, PROMPT_VERSAO, esquema, max_tokens_de_saida};
use crate::resposta::{SugestaoValida, interpretar};
use crate::sugestoes::{
    EstadoDaSugestao, NovaSugestao, atualizar_rodada, gravar, iniciar_rodada,
    usuario_decidiu_o_campo,
};
use crate::tipos::{Alvo, Item};

/// Quantos itens se carregam por vez.
pub const ITENS_POR_PEDACO: usize = 500;

/// A configuração da fila do motor para a triagem: destinatário "triagem", nível Completo
/// (D2), lotes de 25, escalonamento em 0,7.
pub fn configuracao_da_fila(usar_reserva: bool) -> ConfiguracaoDaFila {
    let mut c = ConfiguracaoDaFila::nova("triagem", PROMPT_VERSAO, INSTRUCAO, esquema());
    c.nivel = NivelDeConfianca::Completo;
    c.tamanho_do_lote = ITENS_POR_LOTE;
    c.max_tokens_saida = max_tokens_de_saida(ITENS_POR_LOTE);
    c.usar_reserva = usar_reserva;
    c
}

#[derive(Debug, Clone)]
pub struct ConfigDaRodada {
    pub prompt_versao: String,
    pub itens_por_pedaco: usize,
    /// O piloto (IA-09): não mostra à IA o que o classificador já pôs, não descarta
    /// sugestão igual ao valor atual e grava tudo como `obsoleta`.
    pub piloto: bool,
    /// Onde se vê quais lotes falharam por erro do provedor (`Outro`). Sem ele, o motor
    /// os devolve como "não classificado". Com ele, esses itens ficam adiados.
    pub rastro: Option<RastroDeFalhas>,
}

impl Default for ConfigDaRodada {
    fn default() -> Self {
        Self {
            prompt_versao: PROMPT_VERSAO.to_owned(),
            itens_por_pedaco: ITENS_POR_PEDACO,
            piloto: false,
            rastro: None,
        }
    }
}

/// Embrulha o provedor para lembrar o que falhou por erro transitório (`Outro`). O motor
/// trata essa falha como "não classificado"; a triagem a trata como "adiado": o item não
/// é gravado e volta à fila na próxima rodada. Só resposta inválida repetida vira
/// "não classificado".
#[derive(Debug, Clone, Default)]
pub struct RastroDeFalhas(Arc<Mutex<Vec<String>>>);

struct Rastreado {
    dentro: Arc<dyn sbf_ia::Provedor>,
    rastro: RastroDeFalhas,
}

impl sbf_ia::Provedor for Rastreado {
    fn nome(&self) -> &'static str {
        self.dentro.nome()
    }
    fn pedir(&self, pedido: &sbf_ia::Pedido) -> sbf_ia::ResultadoDeIa<sbf_ia::Resposta> {
        let r = self.dentro.pedir(pedido);
        if let Err(sbf_ia::ErroDeIa::Outro(_)) = &r
            && let Ok(mut v) = self.rastro.0.lock()
        {
            v.push(pedido.entrada.clone());
        }
        r
    }
    fn nova_rodada(&self) {
        self.dentro.nova_rodada();
    }
    fn dentro_do_teto(&self) -> bool {
        self.dentro.dentro_do_teto()
    }
}

impl RastroDeFalhas {
    /// O provedor a passar para a `Fila`.
    pub fn embrulhar(&self, dentro: Arc<dyn sbf_ia::Provedor>) -> Arc<dyn sbf_ia::Provedor> {
        Arc::new(Rastreado {
            dentro,
            rastro: self.clone(),
        })
    }

    fn tomar(&self) -> Vec<String> {
        self.0
            .lock()
            .map_or_else(|_| Vec::new(), |mut v| std::mem::take(&mut *v))
    }
}

/// O item aparece numa entrada que falhou? (compara o corpo do bloco, sem o número.)
fn falhou_por_erro_do_provedor(item: &Item, falhas: &[String]) -> bool {
    let bloco = crate::entrada::bloco_do_item(1, item);
    let corpo = bloco.split_once('\n').map_or(bloco.as_str(), |(_, c)| c);
    falhas.iter().any(|e| e.contains(corpo))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituacaoDaRodada {
    Ociosa,
    Rodando,
    Pausada,
    Concluida,
    Falhou,
    Cancelada,
}

impl SituacaoDaRodada {
    /// O texto de `rodada_ia.situacao`.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::Ociosa | Self::Rodando => "rodando",
            Self::Pausada => "pausada",
            Self::Concluida => "concluida",
            Self::Falhou => "falhou",
            Self::Cancelada => "cancelada",
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
        (Item::Registro(r), "tipo") => s
            .valor
            .as_str()
            .is_some_and(|v| normalizar(&r.tipo) == normalizar(v)),
        _ => false,
    }
}

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

/// O nome que fica em `sugestao_ia.modelo`: o relatório da fila não traz o nome do modelo,
/// só o provedor e o degrau (o nome real está em `saida_ia`).
fn rotulo_do_modelo(provedor: &str, degrau: Degrau) -> String {
    let d = match degrau {
        Degrau::Rapido => "rapido",
        Degrau::Forte => "forte",
    };
    format!("{provedor}:{d}")
}

fn gravar_nao_classificado(
    tx: &rusqlite::Transaction<'_>,
    alvo: &Alvo,
    config: &ConfigDaRodada,
    rodada_id: i64,
) -> Resultado<()> {
    gravar(
        tx,
        &NovaSugestao::nao_classificado(alvo, &config.prompt_versao, rodada_id),
    )?;
    Ok(())
}

/// Grava o que um pedaço produziu. Devolve (itens decididos, sugestões gravadas, não
/// classificados).
fn gravar_pedaco(
    conn: &Connection,
    itens: &[Item],
    relatorio: &Relatorio,
    config: &ConfigDaRodada,
    rodada_id: i64,
    falhas: &[String],
) -> Resultado<(usize, usize, usize)> {
    let estado = if config.piloto {
        EstadoDaSugestao::Obsoleta
    } else {
        EstadoDaSugestao::Aberta
    };
    // Cada lote é interpretado uma vez, com os itens que ele levou.
    let interpretados: Vec<_> = relatorio
        .lotes
        .iter()
        .map(|lote| {
            let do_lote: Vec<Item> = lote.indices.iter().map(|i| itens[*i].clone()).collect();
            interpretar(&lote.json, &do_lote)
        })
        .collect();
    let tx = conn.unchecked_transaction()?;
    let (mut feitos, mut sugestoes, mut nao_classificados) = (0, 0, 0);
    for (i, item) in itens.iter().enumerate() {
        let alvo = item.alvo();
        if relatorio.nao_classificados.contains(&i) {
            if falhou_por_erro_do_provedor(item, falhas) {
                continue; // adiado: volta à fila na próxima rodada
            }
            gravar_nao_classificado(&tx, &alvo, config, rodada_id)?;
            (feitos, nao_classificados) = (feitos + 1, nao_classificados + 1);
            continue;
        }
        let Some(a) = relatorio.atribuicoes[i] else {
            continue; // nunca tentado: volta numa próxima rodada
        };
        let lote = &relatorio.lotes[a.lote];
        let Some(Some(lista)) = interpretados[a.lote].get(a.posicao) else {
            gravar_nao_classificado(&tx, &alvo, config, rodada_id)?;
            (feitos, nao_classificados) = (feitos + 1, nao_classificados + 1);
            continue;
        };
        feitos += 1;
        let modelo = rotulo_do_modelo(&lote.provedor, lote.degrau);
        for s in lista {
            if !config.piloto && igual_ao_atual(item, s) {
                continue;
            }
            // A rodada leva minutos: o usuário pode ter decidido este campo, ou o registro
            // pode ter sido promovido, desde que a fila foi montada. Reconfere aqui, na
            // transação que grava. (O piloto mede o acerto sobre decisões: não filtra.)
            if !config.piloto {
                let promovido =
                    matches!(&alvo, Alvo::Registro(id) if registro_promovido(&tx, *id)?);
                if promovido || usuario_decidiu_o_campo(&tx, &alvo, &s.campo, None)? {
                    continue;
                }
            }
            gravar(
                &tx,
                &NovaSugestao::da_ia(&alvo, s, &modelo, &config.prompt_versao, rodada_id, estado),
            )?;
            sugestoes += 1;
        }
    }
    tx.commit()?;
    Ok((feitos, sugestoes, nao_classificados))
}

/// Roda a triagem sobre `alvos` (a fila de [`crate::fila::selecionar`]).
///
/// `provedor` é o nome para `rodada_ia.provedor`; `ao_abrir` recebe o id da rodada assim que
/// ela nasce (para marcar as saídas no registro). `progresso` é atualizado a cada evento da
/// fila para a janela ler de outra thread. Pausar e cancelar vão pelo `controle` do motor.
///
/// # Errors
/// Falha do SQLite. Falha do provedor **não** é erro: vira a situação no resumo.
#[allow(clippy::too_many_arguments)]
pub fn rodar(
    conn: &Connection,
    fila: &Arc<Fila>,
    provedor: &str,
    alvos: &[Alvo],
    config: &ConfigDaRodada,
    controle: &Controle,
    progresso: &Mutex<Progresso>,
    ao_abrir: &dyn Fn(i64),
) -> Resultado<Progresso> {
    let rodada_id = iniciar_rodada(conn, provedor, config.piloto)?;
    ao_abrir(rodada_id);
    if let Ok(mut p) = progresso.lock() {
        *p = Progresso {
            situacao: SituacaoDaRodada::Rodando,
            rodada_id: Some(rodada_id),
            total: alvos.len(),
            ..Progresso::ocioso()
        };
    }
    let formato = Arc::new(FormatoDaTriagem::novo(Vocabulario::do_banco(conn)?));
    let mut situacao = SituacaoDaRodada::Concluida;
    let mut aviso: Option<String> = None;

    for pedaco in alvos.chunks(config.itens_por_pedaco.max(1)) {
        let itens = carregar_itens(conn, pedaco, !config.piloto)?;
        if itens.is_empty() {
            continue;
        }
        let base = copiar(progresso);
        if let Some(r) = &config.rastro {
            r.tomar();
        }
        let (canal, fio) = fila.iniciar(itens.clone(), Arc::clone(&formato), controle.clone());
        loop {
            match canal.recv_timeout(Duration::from_millis(100)) {
                Ok(evento) => aplicar_evento(progresso, &base, &evento),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if fio.is_finished() {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        for evento in canal.try_iter() {
            aplicar_evento(progresso, &base, &evento);
        }
        let relatorio = fio
            .join()
            .map_err(|_| crate::ErroDaTriagem::Outro("a fila da IA entrou em pânico".to_owned()))?;
        let falhas = config
            .rastro
            .as_ref()
            .map(RastroDeFalhas::tomar)
            .unwrap_or_default();
        let (feitos, sugestoes, nao_classificados) =
            gravar_pedaco(conn, &itens, &relatorio, config, rodada_id, &falhas)?;
        if let Ok(mut p) = progresso.lock() {
            p.processados = base.processados + feitos;
            p.sugestoes = base.sugestoes + sugestoes;
            p.nao_classificados = base.nao_classificados + nao_classificados;
            p.chamadas = base.chamadas + u32::try_from(relatorio.chamadas).unwrap_or(u32::MAX);
            p.subiram_de_degrau = base.subiram_de_degrau + relatorio.escalonados;
            p.tokens_entrada = base.tokens_entrada + relatorio.consumo.tokens_entrada;
            p.tokens_saida = base.tokens_saida + relatorio.consumo.tokens_saida;
            p.custo_dezmilesimos_usd =
                base.custo_dezmilesimos_usd + relatorio.consumo.custo_dezmilesimos_usd;
        }
        gravar_andamento(conn, rodada_id, &copiar(progresso), false)?;
        if let Some(interrupcao) = relatorio.interrupcao {
            (situacao, aviso) = match interrupcao {
                Interrupcao::Cancelada => (
                    SituacaoDaRodada::Cancelada,
                    Some("cancelada por você".to_owned()),
                ),
                Interrupcao::Teto => (
                    SituacaoDaRodada::Pausada,
                    Some("teto de gasto atingido".to_owned()),
                ),
                Interrupcao::LimiteSemFim => (
                    SituacaoDaRodada::Pausada,
                    Some("o limite do provedor não passou; retome mais tarde".to_owned()),
                ),
                Interrupcao::Indisponivel(m) => (
                    SituacaoDaRodada::Falhou,
                    Some(format!("provedor indisponível: {m}")),
                ),
                Interrupcao::FalhaDoRegistro(m) => (
                    SituacaoDaRodada::Falhou,
                    Some(format!("não foi possível registrar o que saiu: {m}")),
                ),
            };
            break;
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

/// Os eventos da fila viram o andamento do pedaço, somado ao que veio antes (`base`).
fn aplicar_evento(progresso: &Mutex<Progresso>, base: &Progresso, evento: &Evento) {
    let Ok(mut p) = progresso.lock() else {
        return;
    };
    match evento {
        Evento::LoteConcluido {
            itens_feitos,
            chamadas,
            consumo,
            ..
        } => {
            p.processados = base.processados + itens_feitos;
            p.chamadas = base.chamadas + u32::try_from(*chamadas).unwrap_or(u32::MAX);
            p.tokens_entrada = base.tokens_entrada + consumo.tokens_entrada;
            p.tokens_saida = base.tokens_saida + consumo.tokens_saida;
            p.custo_dezmilesimos_usd = base.custo_dezmilesimos_usd + consumo.custo_dezmilesimos_usd;
            p.aviso = None;
        }
        Evento::EsperandoLimite { espera_segundos } => {
            p.aviso = Some(format!(
                "limite do provedor; esperando {espera_segundos}s e continuando"
            ));
        }
        Evento::TrocouParaReserva => p.aviso = Some("usando a reserva".to_owned()),
        Evento::Pausada => p.situacao = SituacaoDaRodada::Pausada,
        Evento::Retomada => p.situacao = SituacaoDaRodada::Rodando,
        _ => {}
    }
}
