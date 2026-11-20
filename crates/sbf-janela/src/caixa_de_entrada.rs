//! A Caixa de entrada na janela (CONTEXT.md): a fila dos documentos que o reconhecimento
//! local viu chegar (Telegram, Gmail, pasta, arrastar, `sbf reconhecer`). Aqui moram o
//! estado e a lógica, sem egui desenhando nada (o desenho está em
//! [`crate::caixa_de_entrada_ui`]).
//!
//! Duas listas saem da mesma fila (`sbf_inventario::caixa_de_entrada`):
//!
//! - a **Caixa de entrada** lista o que algum leitor ou regra reconheceu (Pronto, Precisa
//!   revisar, Possível duplicidade): é onde se decidem as sugestões de ligação ao
//!   planejamento;
//! - **A triar** ganha a seção "Sem padrão de reconhecimento" com o que nenhuma regra casou
//!   (Não reconhecido, Protegido por senha): é onde se cria a regra e se reconhece de novo.
//!
//! Escolher uma linha mostra no dock o **mesmo cartão** do painel Reconhecimento (as
//! sugestões são recalculadas na hora, a partir do texto que o inventário já guarda e sem IA)
//! e a Prévia do arquivo. Confirmar uma sugestão segue o caminho de sempre
//! ([`Janela::confirmar_sugestao`]), com Rastro e Desfazer. O que a janela lê e grava da fila
//! passa pela costura [`FonteDaFila`], para os testes e a demonstração usarem uma fictícia.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sbf_inventario::caixa_de_entrada::{
    Contagens, Entrada, Estado, Filtro, Grupo, Origem, Situacao,
};
use sbf_inventario::{Ref, Tipo};
use sbf_reconhecimento::Resultado;

use crate::janela::{Janela, Mensagem};
use crate::navegacao::Destino;
use crate::reconhecimento::{Cartao, SituacaoDoCartao};
use sbf_inventario::consulta::Area;

/// De quanto em quanto tempo as listas são relidas (o worker grava de outro processo).
const RELER_A_CADA: Duration = Duration::from_secs(2);

/// O que a janela lê e pede da fila. Tudo é rápido (SQLite local e texto já extraído).
pub trait FonteDaFila: Send + Sync + std::fmt::Debug {
    /// As linhas da lista, das mais recentes para as mais antigas.
    ///
    /// # Errors
    /// O motivo, sem conteúdo de documento.
    fn listar(&self, filtro: &Filtro) -> Result<Vec<Entrada>, String>;

    /// Os contadores das duas listas.
    ///
    /// # Errors
    /// O motivo.
    fn contagens(&self) -> Result<Contagens, String>;

    /// Resolver, dispensar ou reabrir.
    ///
    /// # Errors
    /// O motivo.
    fn definir_situacao(&self, sha256: &str, situacao: Situacao) -> Result<(), String>;

    /// O cartão do reconhecimento do conteúdo, recalculado agora a partir do texto guardado,
    /// sem IA e **sem mexer na fila**.
    ///
    /// # Errors
    /// O motivo (o arquivo não está mais no cofre, o banco travou).
    fn cartao(&self, sha256: &str) -> Result<Resultado, String>;

    /// "Reconhecer de novo": roda o reconhecimento outra vez (com as regras de agora) e
    /// atualiza a linha da fila. O item pode mudar de lista.
    ///
    /// # Errors
    /// O motivo.
    fn reconhecer_de_novo(&self, sha256: &str) -> Result<Resultado, String>;
}

/// Quais linhas a Caixa de entrada mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Escopo {
    /// O padrão: o que ainda espera uma decisão.
    #[default]
    NaoResolvidos,
    /// Também o que já foi resolvido ou dispensado.
    Todos,
}

impl Escopo {
    pub fn rotulo(self) -> &'static str {
        match self {
            Escopo::NaoResolvidos => "Não resolvidos",
            Escopo::Todos => "Todos",
        }
    }
}

/// O que a interface pede à fila (aplicado depois do desenho, no tique).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PedidoDaFila {
    Selecionar(String),
    Resolver(String),
    Dispensar(String),
    Reabrir(String),
    ReconhecerDeNovo(String),
    Recarregar,
}

/// O estado da fila na janela.
#[derive(Debug, Default)]
pub struct EstadoDaFila {
    fonte: Option<Arc<dyn FonteDaFila>>,
    /// A Caixa de entrada: os reconhecidos.
    pub caixa: Vec<Entrada>,
    /// A seção "Sem padrão de reconhecimento" de A triar.
    pub sem_padrao: Vec<Entrada>,
    pub contagens: Contagens,
    pub escopo: Escopo,
    pub origem: Option<Origem>,
    /// O conteúdo (sha256) da linha escolhida.
    pub selecionada: Option<String>,
    /// A seleção da tabela no momento em que a linha foi escolhida: se ela muda (o usuário
    /// clicou noutra coisa), o dock volta a seguir a tabela.
    selecao_vista: Option<Ref>,
    foco_ativo: bool,
    /// O último erro de leitura ou de ação (sem conteúdo de documento).
    pub erro: Option<String>,
    ultima_leitura: Option<Instant>,
    pub pendentes: Vec<PedidoDaFila>,
    /// A seção de A triar está aberta.
    pub secao_aberta: bool,
}

impl EstadoDaFila {
    pub fn ligada(&self) -> bool {
        self.fonte.is_some()
    }

    /// O filtro da Caixa de entrada no momento.
    pub fn filtro_da_caixa(&self) -> Filtro {
        Filtro {
            grupo: Grupo::Reconhecidos,
            so_abertas: self.escopo == Escopo::NaoResolvidos,
            origem: self.origem,
            limite: 500,
        }
    }

    /// Pede uma releitura no próximo tique.
    pub fn recarregar(&mut self) {
        self.ultima_leitura = None;
    }
}

/// `2026-11-20T14:05:09.123Z` (UTC) como `20/11 11:05` (UTC−3, o fuso do usuário).
pub fn chegada_em_texto(utc: &str) -> String {
    let (Some(dia), Some(hora)) = (utc.get(0..10), utc.get(11..16)) else {
        return utc.to_owned();
    };
    let nums: Option<Vec<i64>> = dia
        .split('-')
        .chain(hora.split(':'))
        .map(|p| p.parse().ok())
        .collect();
    let Some([a, m, d, h, min]) = nums.as_deref().and_then(|v| <[i64; 5]>::try_from(v).ok()) else {
        return utc.to_owned();
    };
    let minutos = h * 60 + min - 3 * 60;
    let (mes, dia, minutos) = if minutos < 0 {
        let (_, m, d) = prev(a, m, d);
        (m, d, minutos + 1440)
    } else {
        (m, d, minutos)
    };
    format!("{dia:02}/{mes:02} {:02}:{:02}", minutos / 60, minutos % 60)
}

/// O dia anterior.
fn prev(a: i64, m: i64, d: i64) -> (i64, i64, i64) {
    if d > 1 {
        return (a, m, d - 1);
    }
    let (pa, pm) = if m == 1 { (a - 1, 12) } else { (a, m - 1) };
    let bissexto = (pa % 4 == 0 && pa % 100 != 0) || pa % 400 == 0;
    let ultimo = match pm {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if bissexto => 29,
        _ => 28,
    };
    (pa, pm, ultimo)
}

impl Janela {
    /// Liga a fila (a real, sobre o `inventario.sqlite`, ou a fictícia da demonstração).
    pub fn ligar_fila(&mut self, fonte: Arc<dyn FonteDaFila>) {
        self.fila.fonte = Some(fonte);
        self.fila.secao_aberta = true;
        self.fila.recarregar();
    }

    /// Os contadores da fila (releituras no máximo a cada poucos segundos); `None` sem fila.
    pub fn contagens_da_fila(&mut self) -> Option<Contagens> {
        self.fila.fonte.as_ref()?;
        self.reler_a_fila(false);
        Some(self.fila.contagens)
    }

    fn reler_a_fila(&mut self, forcar: bool) {
        let Some(fonte) = self.fila.fonte.clone() else {
            return;
        };
        let velha = self
            .fila
            .ultima_leitura
            .is_none_or(|t| t.elapsed() >= RELER_A_CADA);
        if !forcar && !velha {
            return;
        }
        self.fila.ultima_leitura = Some(Instant::now());
        let leitura = (|| -> Result<_, String> {
            Ok((
                fonte.listar(&self.fila.filtro_da_caixa())?,
                fonte.listar(&Filtro::sem_padrao())?,
                fonte.contagens()?,
            ))
        })();
        match leitura {
            Ok((caixa, sem_padrao, contagens)) => {
                self.fila.caixa = caixa;
                self.fila.sem_padrao = sem_padrao;
                self.fila.contagens = contagens;
                self.fila.erro = None;
            }
            Err(e) => self.fila.erro = Some(e),
        }
    }

    /// Chamada a cada quadro (por `arrastar_ui`): aplica o que a interface pediu, relê a fila
    /// quando está velha e resolve o item cujas sugestões acabaram.
    pub fn tique_da_fila(&mut self) {
        if !self.fila.ligada() {
            return;
        }
        let pedidos = std::mem::take(&mut self.fila.pendentes);
        let mexeu = !pedidos.is_empty();
        for p in pedidos {
            self.aplicar_pedido_da_fila(p);
        }
        self.resolver_se_acabou();
        self.reler_a_fila(mexeu);
    }

    fn aplicar_pedido_da_fila(&mut self, pedido: PedidoDaFila) {
        let Some(fonte) = self.fila.fonte.clone() else {
            return;
        };
        match pedido {
            PedidoDaFila::Selecionar(sha) => self.selecionar_na_fila(&sha),
            PedidoDaFila::Resolver(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Resolvida, "Resolvido");
            }
            PedidoDaFila::Dispensar(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Dispensada, "Dispensado");
            }
            PedidoDaFila::Reabrir(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Aberta, "Reaberto");
            }
            PedidoDaFila::ReconhecerDeNovo(sha) => match fonte.reconhecer_de_novo(&sha) {
                Ok(r) => {
                    self.mostrar_cartao_da_fila(&sha, r);
                    self.mensagem = Some(Mensagem {
                        texto: "Reconhecido de novo.".to_owned(),
                        erro: false,
                    });
                }
                Err(e) => self.fila.erro = Some(e),
            },
            PedidoDaFila::Recarregar => self.fila.recarregar(),
        }
    }

    fn mudar_situacao(&mut self, fonte: &dyn FonteDaFila, sha: &str, s: Situacao, feito: &str) {
        match fonte.definir_situacao(sha, s) {
            Ok(()) => {
                if self.fila.selecionada.as_deref() == Some(sha) && s != Situacao::Aberta {
                    self.soltar_a_selecao_da_fila();
                }
                self.mensagem = Some(Mensagem {
                    texto: format!("{feito}."),
                    erro: false,
                });
            }
            Err(e) => self.fila.erro = Some(e),
        }
    }

    fn soltar_a_selecao_da_fila(&mut self) {
        self.fila.selecionada = None;
        self.fila.foco_ativo = false;
        self.reconhecimento.da_caixa = None;
    }

    /// Escolhe uma linha: o dock passa a mostrar o cartão e a Prévia dela.
    fn selecionar_na_fila(&mut self, sha: &str) {
        let Some(fonte) = self.fila.fonte.clone() else {
            return;
        };
        match fonte.cartao(sha) {
            Ok(r) => self.mostrar_cartao_da_fila(sha, r),
            Err(e) => self.fila.erro = Some(e),
        }
    }

    fn mostrar_cartao_da_fila(&mut self, sha: &str, resultado: Resultado) {
        let cartao = Cartao::da_fila(resultado);
        let cartoes = &mut self.reconhecimento.cartoes;
        // Um lugar só para o cartão da fila: trocar de linha reaproveita o mesmo.
        let vaga = self
            .reconhecimento
            .da_caixa
            .filter(|i| cartoes.get(*i).is_some_and(|c| !c.em_andamento()));
        let indice = match vaga {
            Some(i) => {
                cartoes[i] = cartao;
                i
            }
            None => {
                cartoes.push(cartao);
                cartoes.len() - 1
            }
        };
        self.reconhecimento.da_caixa = Some(indice);
        self.guardar_nomes_das_regras(indice);
        self.fila.selecionada = Some(sha.to_owned());
        self.fila.selecao_vista = self.centro.selecao.foco.clone();
        self.fila.foco_ativo = true;
        self.reconhecimento.painel_pedido = true;
        self.fila.recarregar();
    }

    /// Todas as sugestões do cartão foram confirmadas ou ignoradas, e alguma foi confirmada:
    /// o item está resolvido e sai da lista padrão.
    fn resolver_se_acabou(&mut self) {
        let (Some(sha), Some(i)) = (self.fila.selecionada.clone(), self.reconhecimento.da_caixa)
        else {
            return;
        };
        let Some(fonte) = self.fila.fonte.clone() else {
            return;
        };
        let Some(c) = self.reconhecimento.cartoes.get(i) else {
            return;
        };
        let SituacaoDoCartao::Pronto(r) = &c.situacao else {
            return;
        };
        let todas = !r.sugestoes.is_empty()
            && (0..r.sugestoes.len())
                .all(|s| c.confirmadas.contains(&s) || c.ignoradas.contains(&s))
            && !c.confirmadas.is_empty();
        if todas {
            self.mudar_situacao(&*fonte, &sha, Situacao::Resolvida, "Resolvido");
            self.fila.recarregar();
        }
    }

    /// O item que o dock mostra quando uma linha da fila está escolhida: o achado dela.
    pub(crate) fn foco_da_fila(&self) -> Option<Ref> {
        self.cartao_da_fila_em_foco()?;
        let sha = self.fila.selecionada.as_deref()?;
        Some(Ref::nova(Tipo::Achado, sha))
    }

    /// O índice do cartão da fila quando ele é o que o dock deve mostrar: a linha segue
    /// escolhida, a tabela não mudou de seleção e o centro ainda é a Caixa de entrada ou A
    /// triar.
    pub(crate) fn cartao_da_fila_em_foco(&self) -> Option<usize> {
        if !self.fila.foco_ativo || self.fila.selecionada.is_none() {
            return None;
        }
        if self.fila.selecao_vista != self.centro.selecao.foco {
            return None;
        }
        let atual = self.navegacao.atual();
        if !matches!(
            atual,
            Destino::AreaFutura("caixa") | Destino::Area(Area::ATriar)
        ) {
            return None;
        }
        self.reconhecimento.da_caixa
    }

    /// Para os testes de fora do crate: o índice do cartão da fila quando ele está em foco.
    #[doc(hidden)]
    pub fn cartao_da_fila_em_foco_para_testes(&self) -> Option<usize> {
        self.cartao_da_fila_em_foco()
    }

    /// Soltar um arquivo na janela volta o dock para os cartões do arrastar.
    pub(crate) fn largar_o_foco_da_fila(&mut self) {
        self.fila.foco_ativo = false;
    }
}

// ---------------------------------------------------------------------------------------
// A fila fictícia (demonstração e testes)
// ---------------------------------------------------------------------------------------

/// Uma fila em memória: as linhas e, para cada conteúdo, o resultado que o cartão mostra.
#[derive(Debug, Default)]
pub struct FilaFicticia {
    dentro: Mutex<Dentro>,
}

#[derive(Debug, Default)]
struct Dentro {
    entradas: Vec<Entrada>,
    resultados: Vec<(String, Resultado)>,
    /// O que "Reconhecer de novo" devolve no lugar do resultado e do estado de antes.
    depois: Vec<(String, Resultado, Estado)>,
}

impl FilaFicticia {
    pub fn nova() -> Self {
        Self::default()
    }

    /// Acrescenta uma linha e o resultado do cartão dela.
    #[must_use]
    pub fn com(self, entrada: Entrada, resultado: Resultado) -> Self {
        if let Ok(mut d) = self.dentro.lock() {
            d.resultados.push((entrada.content_hash.clone(), resultado));
            d.entradas.push(entrada);
        }
        self
    }

    /// Define o que "Reconhecer de novo" passa a devolver para o conteúdo (e o novo estado).
    #[must_use]
    pub fn reconhecendo_depois_como(self, sha: &str, resultado: Resultado, estado: Estado) -> Self {
        if let Ok(mut d) = self.dentro.lock() {
            d.depois.push((sha.to_owned(), resultado, estado));
        }
        self
    }

    /// A situação atual da linha (para os testes).
    pub fn situacao_de(&self, sha: &str) -> Option<Situacao> {
        let d = self.dentro.lock().ok()?;
        d.entradas
            .iter()
            .find(|e| e.content_hash == sha)
            .map(|e| e.situacao)
    }

    /// O estado atual da linha (para os testes).
    pub fn estado_de(&self, sha: &str) -> Option<Estado> {
        let d = self.dentro.lock().ok()?;
        d.entradas
            .iter()
            .find(|e| e.content_hash == sha)
            .map(|e| e.estado)
    }
}

/// Uma linha fictícia da fila.
pub fn entrada_ficticia(
    sha: &str,
    nome: &str,
    chegou_em: &str,
    origem: Origem,
    tipo: Option<&str>,
    resumo: &str,
    estado: Estado,
) -> Entrada {
    Entrada {
        content_hash: sha.to_owned(),
        nome: nome.to_owned(),
        chegou_em: chegou_em.to_owned(),
        atualizado_em: chegou_em.to_owned(),
        origem,
        tipo: tipo.map(str::to_owned),
        resumo: Some(resumo.to_owned()),
        estado,
        situacao: Situacao::Aberta,
        vezes: 1,
    }
}

impl FonteDaFila for FilaFicticia {
    fn listar(&self, f: &Filtro) -> Result<Vec<Entrada>, String> {
        let d = self.dentro.lock().map_err(|_| "fila travada".to_owned())?;
        let mut v: Vec<Entrada> = d
            .entradas
            .iter()
            .filter(|e| match f.grupo {
                Grupo::Reconhecidos => e.estado.e_reconhecido() || e.estado == Estado::Processando,
                Grupo::SemPadrao => e.estado.sem_padrao(),
                Grupo::Todos => true,
            })
            .filter(|e| !f.so_abertas || e.situacao == Situacao::Aberta)
            .filter(|e| f.origem.is_none_or(|o| o == e.origem))
            .cloned()
            .collect();
        v.sort_by(|a, b| b.chegou_em.cmp(&a.chegou_em));
        Ok(v)
    }

    fn contagens(&self) -> Result<Contagens, String> {
        let d = self.dentro.lock().map_err(|_| "fila travada".to_owned())?;
        let abertas = d.entradas.iter().filter(|e| e.situacao == Situacao::Aberta);
        let mut c = Contagens::default();
        for e in abertas {
            if e.estado.e_reconhecido() {
                c.reconhecidos += 1;
            } else if e.estado.sem_padrao() {
                c.sem_padrao += 1;
            }
        }
        Ok(c)
    }

    fn definir_situacao(&self, sha: &str, s: Situacao) -> Result<(), String> {
        let mut d = self.dentro.lock().map_err(|_| "fila travada".to_owned())?;
        match d.entradas.iter_mut().find(|e| e.content_hash == sha) {
            Some(e) => {
                e.situacao = s;
                Ok(())
            }
            None => Err("o item não está na fila".to_owned()),
        }
    }

    fn cartao(&self, sha: &str) -> Result<Resultado, String> {
        let d = self.dentro.lock().map_err(|_| "fila travada".to_owned())?;
        d.resultados
            .iter()
            .find(|(s, _)| s == sha)
            .map(|(_, r)| r.clone())
            .ok_or_else(|| "o arquivo não está mais no cofre".to_owned())
    }

    fn reconhecer_de_novo(&self, sha: &str) -> Result<Resultado, String> {
        let mut d = self.dentro.lock().map_err(|_| "fila travada".to_owned())?;
        let (r, estado) = match d.depois.iter().find(|(s, _, _)| s == sha) {
            Some((_, r, e)) => (r.clone(), Some(*e)),
            None => (
                d.resultados
                    .iter()
                    .find(|(s, _)| s == sha)
                    .map(|(_, r)| r.clone())
                    .ok_or_else(|| "o arquivo não está mais no cofre".to_owned())?,
                None,
            ),
        };
        if let Some(e) = d.entradas.iter_mut().find(|e| e.content_hash == sha) {
            if let Some(estado) = estado {
                e.estado = estado;
            }
            e.vezes += 1;
        }
        if let Some(slot) = d.resultados.iter_mut().find(|(s, _)| s == sha) {
            slot.1 = r.clone();
        }
        Ok(r)
    }
}

impl Cartao {
    /// O cartão de um item da fila: o resultado já pronto, sem etapas a anunciar.
    pub(crate) fn da_fila(resultado: Resultado) -> Self {
        let mut c = Cartao::novo(std::path::PathBuf::from(&resultado.nome));
        c.nome = resultado.nome.clone();
        c.situacao = SituacaoDoCartao::Pronto(Box::new(resultado));
        c
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_chegada_vai_para_o_fuso_do_usuario() {
        assert_eq!(chegada_em_texto("2026-11-20T14:05:09.123Z"), "20/11 11:05");
        // Antes das 03:00 UTC vira o dia anterior.
        assert_eq!(chegada_em_texto("2026-11-20T01:30:00.000Z"), "19/11 22:30");
        assert_eq!(chegada_em_texto("2026-03-01T02:00:00.000Z"), "28/02 23:00");
        assert_eq!(chegada_em_texto("2026-01-01T00:10:00.000Z"), "31/12 21:10");
        assert_eq!(chegada_em_texto("lixo"), "lixo");
    }
}
