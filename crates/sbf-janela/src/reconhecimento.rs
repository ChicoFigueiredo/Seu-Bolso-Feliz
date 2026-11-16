//! Arrastar um arquivo para a janela (spec 11): a fila, a thread do reconhecimento e o que
//! acontece quando o usuário confirma uma sugestão.
//!
//! Aqui mora o estado e a lógica, sem egui desenhando nada (o desenho está em
//! [`crate::reconhecimento_ui`]). Regras:
//!
//! - cada arquivo vira um cartão; o reconhecimento roda **um por vez**, numa thread, e a
//!   interface só consulta o canal a cada quadro;
//! - pasta é recusada; arquivo repetido na fila é ignorado; sem [`Reconhecedor`] ligado, nada
//!   entra na fila e a janela diz isso;
//! - **nada muda no planejamento sem o clique**: [`Janela::confirmar_sugestao`] é o único
//!   caminho que escreve, com `Autor::Usuario`.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

use sbf_planejamento::{Alteracao, Autor};
use sbf_reconhecimento::{
    Andamento, ErroDoReconhecimento, Etapa, Reconhecedor, Resultado, Situacao, Sugestao,
};

use crate::controle_ui::mensagem_de_erro;
use crate::janela::{Janela, Mensagem};

/// Em que pé está o cartão de um arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SituacaoDoCartao {
    /// Na fila, esperando a vez.
    Aguardando,
    Rodando,
    Pronto(Box<Resultado>),
    /// O reconhecimento parou (o motivo, em português, sem conteúdo do documento).
    Falhou(String),
    Cancelado,
}

/// O cartão de um arquivo solto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartao {
    pub caminho: PathBuf,
    /// Só o nome do arquivo, sem a pasta.
    pub nome: String,
    /// As seis etapas, na ordem; `None` até a etapa anunciar algo.
    pub etapas: Vec<(Etapa, Option<Situacao>)>,
    pub situacao: SituacaoDoCartao,
    /// Os índices das sugestões já confirmadas.
    pub confirmadas: Vec<usize>,
    /// Os índices das sugestões ignoradas.
    pub ignoradas: Vec<usize>,
    /// O erro da última tentativa de confirmar (índice da sugestão, motivo).
    pub erro_de_sugestao: Option<(usize, String)>,
    /// A marcação de eventuais que a pessoa deixou em cada sugestão de rubricas (índice da
    /// sugestão): `true` = só deste mês. Quem não mexeu não aparece: vale a inicial.
    pub marcacoes: BTreeMap<usize, Vec<bool>>,
}

impl Cartao {
    fn novo(caminho: PathBuf) -> Self {
        let nome = caminho.file_name().map_or_else(
            || "arquivo".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        Self {
            caminho,
            nome,
            etapas: Etapa::TODAS.iter().map(|e| (*e, None)).collect(),
            situacao: SituacaoDoCartao::Aguardando,
            confirmadas: Vec::new(),
            ignoradas: Vec::new(),
            erro_de_sugestao: None,
            marcacoes: BTreeMap::new(),
        }
    }

    /// A marcação de eventuais da sugestão de rubricas `sugestao`: a da pessoa, ou a inicial.
    pub fn eventuais_da(&self, sugestao: usize) -> Option<Vec<bool>> {
        let Sugestao::UsarRubricas { uso, .. } = self.resultado()?.sugestoes.get(sugestao)? else {
            return None;
        };
        Some(
            self.marcacoes
                .get(&sugestao)
                .cloned()
                .unwrap_or_else(|| uso.eventuais_iniciais()),
        )
    }

    /// O cartão ainda ocupa a fila (espera ou roda).
    pub fn em_andamento(&self) -> bool {
        matches!(
            self.situacao,
            SituacaoDoCartao::Aguardando | SituacaoDoCartao::Rodando
        )
    }

    /// A situação da etapa, se ela já anunciou alguma.
    pub fn situacao_da_etapa(&self, etapa: Etapa) -> Option<&Situacao> {
        self.etapas
            .iter()
            .find(|(e, _)| *e == etapa)
            .and_then(|(_, s)| s.as_ref())
    }

    /// O resultado, quando o reconhecimento terminou.
    pub fn resultado(&self) -> Option<&Resultado> {
        match &self.situacao {
            SituacaoDoCartao::Pronto(r) => Some(r),
            _ => None,
        }
    }
}

enum Aviso {
    Andamento(Andamento),
    Fim(Box<Result<Resultado, ErroDoReconhecimento>>),
}

struct Execucao {
    indice: usize,
    canal: Receiver<Aviso>,
    cancelar: Arc<AtomicBool>,
    // Guardada só para a thread não ficar solta; ela termina sozinha.
    _thread: JoinHandle<()>,
}

impl std::fmt::Debug for Execucao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execucao")
            .field("indice", &self.indice)
            .finish_non_exhaustive()
    }
}

/// O estado do arrastar e do reconhecimento na janela.
#[derive(Debug, Default)]
pub struct Reconhecimento {
    reconhecedor: Option<Arc<dyn Reconhecedor>>,
    pub cartoes: Vec<Cartao>,
    fila: VecDeque<usize>,
    execucao: Option<Execucao>,
    /// Há arquivo pairando sobre a janela: a camada "Solte para reconhecer".
    pub pairando: bool,
    /// O painel deve aparecer (ao soltar); o app consome isto uma vez.
    pub painel_pedido: bool,
    /// Rola a lista de cartões até este ponto no próximo quadro (as capturas usam, para
    /// mostrar as rubricas); a interface consome uma vez.
    pub rolar_para: Option<f32>,
}

impl Reconhecimento {
    /// Há reconhecedor ligado.
    pub fn ligado(&self) -> bool {
        self.reconhecedor.is_some()
    }

    /// Alguma coisa ainda roda ou espera.
    pub fn ocupado(&self) -> bool {
        self.execucao.is_some() || !self.fila.is_empty()
    }
}

/// O texto quando a janela não tem reconhecedor.
pub const NAO_LIGADO: &str = "O reconhecimento não está ligado nesta janela";

impl Janela {
    /// Liga o reconhecedor (o real, do motor, ou o fictício da demonstração).
    pub fn ligar_reconhecedor(&mut self, reconhecedor: Arc<dyn Reconhecedor>) {
        self.reconhecimento.reconhecedor = Some(reconhecedor);
    }

    /// Arquivos soltos na janela. `ctx` acorda a interface quando a thread anuncia algo.
    pub fn soltar_arquivos(&mut self, caminhos: Vec<PathBuf>, ctx: Option<&egui::Context>) {
        if caminhos.is_empty() {
            return;
        }
        if !self.reconhecimento.ligado() {
            self.mensagem = Some(Mensagem {
                texto: format!("{NAO_LIGADO}."),
                erro: true,
            });
            return;
        }
        let mut pastas = 0usize;
        let mut aceitos = 0usize;
        for caminho in caminhos {
            if caminho.is_dir() {
                pastas += 1;
                continue;
            }
            let repetido = self
                .reconhecimento
                .cartoes
                .iter()
                .any(|c| c.em_andamento() && c.caminho == caminho);
            if repetido {
                continue;
            }
            self.reconhecimento.cartoes.push(Cartao::novo(caminho));
            let indice = self.reconhecimento.cartoes.len() - 1;
            self.reconhecimento.fila.push_back(indice);
            aceitos += 1;
        }
        if pastas > 0 {
            self.mensagem = Some(Mensagem {
                texto: if pastas == 1 {
                    "Pasta não vale: solte arquivos, não pastas.".to_owned()
                } else {
                    format!("{pastas} pastas não valem: solte arquivos, não pastas.")
                },
                erro: true,
            });
        }
        if aceitos > 0 {
            self.reconhecimento.painel_pedido = true;
        }
        self.tique_do_reconhecimento(ctx);
    }

    /// Lê o canal da thread, aplica o que chegou e começa o próximo da fila. Devolve `true`
    /// enquanto ainda há trabalho (quem desenha pede novo quadro).
    pub fn tique_do_reconhecimento(&mut self, ctx: Option<&egui::Context>) -> bool {
        if let Some(exec) = self.reconhecimento.execucao.as_ref() {
            let indice = exec.indice;
            let mut terminou = false;
            loop {
                match exec.canal.try_recv() {
                    Ok(Aviso::Andamento(a)) => {
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            if let Some(slot) = c.etapas.iter_mut().find(|(e, _)| *e == a.etapa) {
                                slot.1 = Some(a.situacao);
                            }
                            c.situacao = SituacaoDoCartao::Rodando;
                        }
                    }
                    Ok(Aviso::Fim(r)) => {
                        let r = *r;
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            c.situacao = match r {
                                Ok(resultado) => SituacaoDoCartao::Pronto(Box::new(resultado)),
                                Err(ErroDoReconhecimento::Cancelado) => SituacaoDoCartao::Cancelado,
                                Err(e) => SituacaoDoCartao::Falhou(e.to_string()),
                            };
                        }
                        terminou = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            c.situacao = SituacaoDoCartao::Falhou(
                                "O reconhecimento parou de repente.".to_owned(),
                            );
                        }
                        terminou = true;
                        break;
                    }
                }
            }
            if terminou {
                self.reconhecimento.execucao = None;
            }
        }
        if self.reconhecimento.execucao.is_none() {
            self.iniciar_o_proximo(ctx);
        }
        self.reconhecimento.ocupado()
    }

    fn iniciar_o_proximo(&mut self, ctx: Option<&egui::Context>) {
        let Some(reconhecedor) = self.reconhecimento.reconhecedor.clone() else {
            return;
        };
        while let Some(indice) = self.reconhecimento.fila.pop_front() {
            let Some(cartao) = self.reconhecimento.cartoes.get_mut(indice) else {
                continue;
            };
            // Cancelado na espera: pula.
            if cartao.situacao != SituacaoDoCartao::Aguardando {
                continue;
            }
            cartao.situacao = SituacaoDoCartao::Rodando;
            let caminho = cartao.caminho.clone();
            let (envio, canal) = channel();
            let cancelar = Arc::new(AtomicBool::new(false));
            let da_thread = Arc::clone(&cancelar);
            let ctx = ctx.cloned();
            let acordar = move || {
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            };
            let thread = std::thread::spawn(move || {
                let envio_do_andamento = envio.clone();
                let acordar_andamento = acordar.clone();
                let r = reconhecedor.reconhecer(
                    &caminho,
                    &mut |a| {
                        let _ = envio_do_andamento.send(Aviso::Andamento(a));
                        acordar_andamento();
                    },
                    &da_thread,
                );
                let _ = envio.send(Aviso::Fim(Box::new(r)));
                acordar();
            });
            self.reconhecimento.execucao = Some(Execucao {
                indice,
                canal,
                cancelar,
                _thread: thread,
            });
            return;
        }
    }

    /// Cancela o arquivo `indice`: na espera, sai da fila; rodando, a thread é avisada e o
    /// reconhecedor para entre etapas.
    pub fn cancelar_reconhecimento(&mut self, indice: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(indice) else {
            return;
        };
        match c.situacao {
            SituacaoDoCartao::Aguardando => c.situacao = SituacaoDoCartao::Cancelado,
            SituacaoDoCartao::Rodando => {
                if let Some(e) = self
                    .reconhecimento
                    .execucao
                    .as_ref()
                    .filter(|e| e.indice == indice)
                {
                    e.cancelar.store(true, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }

    /// Tira da lista os cartões que já terminaram (os que esperam ou rodam ficam).
    pub fn limpar_reconhecimento(&mut self) {
        let r = &mut self.reconhecimento;
        // Os índices da fila e da execução mudam quando a lista encolhe: só limpa sem fila.
        if r.execucao.is_some() || !r.fila.is_empty() {
            return;
        }
        r.cartoes.clear();
    }

    /// Ignorar uma sugestão: some o botão, nada muda no planejamento.
    pub fn ignorar_sugestao(&mut self, cartao: usize, sugestao: usize) {
        if let Some(c) = self.reconhecimento.cartoes.get_mut(cartao)
            && !c.ignoradas.contains(&sugestao)
            && !c.confirmadas.contains(&sugestao)
        {
            c.ignoradas.push(sugestao);
        }
    }

    /// Troca a rubrica `rubrica` da sugestão de rubricas entre "fixa" e "só deste mês".
    /// Só vale antes de confirmar; nada é gravado.
    pub fn alternar_eventual(&mut self, cartao: usize, sugestao: usize, rubrica: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        if c.confirmadas.contains(&sugestao) {
            return;
        }
        let Some(mut marcacao) = c.eventuais_da(sugestao) else {
            return;
        };
        let Some(m) = marcacao.get_mut(rubrica) else {
            return;
        };
        *m = !*m;
        c.marcacoes.insert(sugestao, marcacao);
    }

    /// Confirmar uma sugestão: o único caminho que escreve no planejamento (`Autor::Usuario`).
    /// Confirmar de novo a mesma sugestão não faz nada. As rubricas aplicam uma composição
    /// por vez, cada uma com a versão do recorrente lida logo antes.
    pub fn confirmar_sugestao(&mut self, cartao: usize, sugestao: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        if c.confirmadas.contains(&sugestao) {
            return;
        }
        c.erro_de_sugestao = None;
        let marcacao = c.eventuais_da(sugestao);
        let Some(resultado) = c.resultado() else {
            return;
        };
        let Some(s) = resultado.sugestoes.get(sugestao).cloned() else {
            return;
        };
        let (sha, nome) = (resultado.sha256.clone(), resultado.nome.clone());
        let Some(pl) = self.planejamento.as_mut() else {
            c.erro_de_sugestao = Some((
                sugestao,
                "O planejamento ainda não está ligado a esta janela.".to_owned(),
            ));
            return;
        };
        let plan = pl.0.as_mut();
        let mut resultado_final = Ok(());
        for mut alteracao in s.alteracoes(&sha, &nome, 0, marcacao.as_deref()) {
            // A versão do recorrente lida agora: se mudou desde o reconhecimento, o conflito
            // aparece no cartão em vez de sobrescrever.
            if let (Alteracao::DefinirComposicao { versao_base, .. }, Some(id)) =
                (&mut alteracao, s.recorrente_para_versao())
            {
                match plan.recorrente(id) {
                    Ok(Some(r)) => *versao_base = r.versao_do_registro,
                    Ok(None) => *versao_base = 0,
                    Err(e) => {
                        resultado_final = Err(mensagem_de_erro(&e));
                        break;
                    }
                }
            }
            if let Err(e) = plan.aplicar(alteracao, Autor::Usuario) {
                resultado_final = Err(mensagem_de_erro(&e));
                break;
            }
        }
        self.controle.recarregar();
        match resultado_final {
            Ok(()) => {
                c.confirmadas.push(sugestao);
                self.controle.mensagem = Some((format!("Feito: {}.", s.rotulo()), false));
            }
            Err(e) => c.erro_de_sugestao = Some((sugestao, e)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
    use crate::reconhecimento_ficticio::{ID_DO_SALARIO, reconhecedor_do_contracheque};
    use sbf_planejamento::{AlvoDoDocumento, Competencia, Data, IdOcorrencia};
    use sbf_reconhecimento::{ReconhecedorFicticio, Reconhecido, Sugestao};
    use std::time::{Duration, Instant};

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn janela_com_tudo() -> (Janela, Arc<ReconhecedorFicticio>) {
        let mut j = Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        let r = Arc::new(reconhecedor_do_contracheque());
        j.ligar_reconhecedor(r.clone());
        (j, r)
    }

    fn esperar(j: &mut Janela) {
        let limite = Instant::now() + Duration::from_secs(10);
        while j.tique_do_reconhecimento(None) {
            assert!(Instant::now() < limite, "o reconhecimento não terminou");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn arquivo(dir: &tempfile::TempDir, nome: &str) -> PathBuf {
        let p = dir.path().join(nome);
        std::fs::write(&p, b"ficticio").expect("escrever");
        p
    }

    fn alvo_de_outubro() -> AlvoDoDocumento {
        AlvoDoDocumento::Ocorrencia {
            id: IdOcorrencia {
                recorrente: ID_DO_SALARIO.to_owned(),
                competencia: Competencia { ano: 2026, mes: 10 },
                parcela: None,
            },
        }
    }

    fn documentos_do_alvo(j: &Janela) -> usize {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .documentos(&alvo_de_outubro())
            .expect("documentos")
            .iter()
            .filter(|d| !d.desvinculado)
            .count()
    }

    #[test]
    fn soltar_passa_pela_fila_pelas_etapas_e_chega_ao_resultado() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let a = arquivo(&dir, "contracheque.pdf");
        j.soltar_arquivos(vec![a.clone()], None);
        assert!(j.reconhecimento.painel_pedido);
        assert_eq!(j.reconhecimento.cartoes.len(), 1);
        esperar(&mut j);
        let c = &j.reconhecimento.cartoes[0];
        assert_eq!(c.nome, "contracheque.pdf");
        for etapa in Etapa::TODAS {
            assert!(
                matches!(
                    c.situacao_da_etapa(etapa),
                    Some(Situacao::Feita | Situacao::Pulada(_))
                ),
                "etapa sem resposta: {etapa:?}"
            );
        }
        let res = c.resultado().expect("resultado");
        assert!(matches!(res.reconhecido, Reconhecido::Contracheque { .. }));
        assert_eq!(res.sugestoes.len(), 3);
        assert_eq!(r.recebidos.lock().expect("trava").as_slice(), &[a]);
    }

    #[test]
    fn a_fila_e_sequencial_e_ignora_repetido() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let a = arquivo(&dir, "a.pdf");
        let b = arquivo(&dir, "b.pdf");
        j.soltar_arquivos(vec![a.clone(), b.clone(), a.clone()], None);
        assert_eq!(j.reconhecimento.cartoes.len(), 2);
        esperar(&mut j);
        assert!(
            j.reconhecimento
                .cartoes
                .iter()
                .all(|c| c.resultado().is_some())
        );
        assert_eq!(r.recebidos.lock().expect("trava").as_slice(), &[a, b]);
    }

    #[test]
    fn pasta_e_recusada_com_mensagem() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![dir.path().to_path_buf()], None);
        assert!(j.reconhecimento.cartoes.is_empty());
        assert!(!j.reconhecimento.painel_pedido);
        let m = j.mensagem.as_ref().expect("mensagem");
        assert!(m.erro && m.texto.contains("Pasta"));
        assert!(r.recebidos.lock().expect("trava").is_empty());
    }

    #[test]
    fn sem_reconhecedor_diz_que_nao_esta_ligado() {
        let mut j = Janela::nova();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "a.pdf")], None);
        assert!(j.reconhecimento.cartoes.is_empty());
        let m = j.mensagem.as_ref().expect("mensagem");
        assert!(m.erro && m.texto.starts_with(NAO_LIGADO));
    }

    #[test]
    fn confirmar_vincular_cria_o_documento_no_alvo_e_duas_vezes_nao_duplica() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let antes = documentos_do_alvo(&j);
        let i = j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::Vincular { .. }))
            .expect("vincular");
        j.confirmar_sugestao(0, i);
        assert_eq!(documentos_do_alvo(&j), antes + 1);
        assert!(j.reconhecimento.cartoes[0].confirmadas.contains(&i));
        j.confirmar_sugestao(0, i);
        assert_eq!(documentos_do_alvo(&j), antes + 1);
        assert!(j.reconhecimento.cartoes[0].erro_de_sugestao.is_none());
    }

    fn indice_de_rubricas(j: &Janela) -> usize {
        j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::UsarRubricas { .. }))
            .expect("usar rubricas")
    }

    fn uso_do_cartao(j: &Janela) -> sbf_reconhecimento::UsoDeRubricas {
        match &j.reconhecimento.cartoes[0]
            .resultado()
            .expect("r")
            .sugestoes[indice_de_rubricas(j)]
        {
            Sugestao::UsarRubricas { uso, .. } => (**uso).clone(),
            _ => unreachable!(),
        }
    }

    fn composicao_em(j: &Janela, desde: Competencia) -> Option<sbf_planejamento::Composicao> {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .composicao(ID_DO_SALARIO, desde)
            .expect("composição")
    }

    #[test]
    fn confirmar_usar_rubricas_define_as_duas_composicoes() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let uso = uso_do_cartao(&j);
        let esperadas = uso.composicoes(&uso.eventuais_iniciais());
        assert_eq!(esperadas.len(), 2);
        let i = indice_de_rubricas(&j);
        j.confirmar_sugestao(0, i);
        assert_eq!(
            j.reconhecimento.cartoes[0].erro_de_sugestao, None,
            "a confirmação falhou"
        );
        for e in &esperadas {
            let gravada = composicao_em(&j, e.desde).expect("existe");
            assert_eq!(gravada.desde, e.desde);
            assert_eq!(gravada.rubricas, e.rubricas);
            assert_eq!(gravada.documento, e.documento);
        }
    }

    #[test]
    fn trocar_a_marcacao_antes_de_confirmar_muda_o_que_e_gravado() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let uso = uso_do_cartao(&j);
        let i = indice_de_rubricas(&j);
        let inicial = uso.eventuais_iniciais();
        let fixas_antes = uso.composicoes(&inicial)[1].rubricas.len();
        // O prêmio (eventual) passa a fixo.
        let premio = uso
            .contracheque
            .rubricas
            .iter()
            .position(|r| r.nome == "PREMIO CAIXA")
            .expect("prêmio");
        assert!(inicial[premio]);
        j.alternar_eventual(0, i, premio);
        assert!(
            !j.reconhecimento.cartoes[0]
                .eventuais_da(i)
                .expect("marcação")[premio]
        );
        j.confirmar_sugestao(0, i);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        let fixas = composicao_em(&j, Competencia { ano: 2026, mes: 11 }).expect("fixas");
        assert_eq!(fixas.rubricas.len(), fixas_antes + 1);
        assert!(fixas.rubricas.iter().any(|r| r.nome == "PREMIO CAIXA"));
    }

    #[test]
    fn alternar_nao_grava_nada_e_nao_vale_depois_de_confirmar() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = indice_de_rubricas(&j);
        let antes = composicao_em(&j, Competencia { ano: 2026, mes: 10 });
        j.alternar_eventual(0, i, 0);
        assert_eq!(composicao_em(&j, Competencia { ano: 2026, mes: 10 }), antes);
        j.confirmar_sugestao(0, i);
        let marcacoes = j.reconhecimento.cartoes[0].marcacoes.clone();
        j.alternar_eventual(0, i, 1);
        assert_eq!(j.reconhecimento.cartoes[0].marcacoes, marcacoes);
    }

    #[test]
    fn confirmar_conciliar_grava_a_conciliacao_confirmada_do_mes() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::Conciliar { .. }))
            .expect("conciliar");
        j.confirmar_sugestao(0, i);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        assert!(j.reconhecimento.cartoes[0].confirmadas.contains(&i));
        let o = j
            .planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .proximas(ID_DO_SALARIO, Competencia { ano: 2026, mes: 10 }, 1, hoje())
            .expect("proximas");
        assert_eq!(o[0].conciliado, Some(sbf_planejamento::Centavos(2_460_522)));
        let c = j
            .planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .sugestao_de_conciliacao(&o[0].id)
            .expect("sugestao");
        assert!(c.is_none(), "a conciliação confirmada não é mais sugestão");
    }

    #[test]
    fn conflito_de_versao_aparece_no_cartao_e_nao_marca_feita() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        // A sugestão traz um recorrente que não existe: a escrita é recusada.
        if let SituacaoDoCartao::Pronto(r) = &mut j.reconhecimento.cartoes[0].situacao {
            for s in &mut r.sugestoes {
                if let Sugestao::UsarRubricas { uso, .. } = s {
                    uso.recorrente = "rec_que_nao_existe".to_owned();
                }
            }
        }
        let i = indice_de_rubricas(&j);
        j.confirmar_sugestao(0, i);
        let c = &j.reconhecimento.cartoes[0];
        assert!(c.confirmadas.is_empty());
        assert!(matches!(&c.erro_de_sugestao, Some((k, m)) if *k == i && !m.is_empty()));
    }

    #[test]
    fn ignorar_nao_escreve_nada() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let antes = documentos_do_alvo(&j);
        j.ignorar_sugestao(0, 0);
        assert_eq!(documentos_do_alvo(&j), antes);
        assert_eq!(j.reconhecimento.cartoes[0].ignoradas, vec![0]);
        // Ignorada, a confirmação por engano ainda é possível via API, mas a interface
        // esconde os botões; o que importa: ignorar sozinho nunca grava.
    }

    #[test]
    fn falha_do_reconhecedor_vira_cartao_falhou() {
        let mut j = Janela::nova();
        j.ligar_reconhecedor(Arc::new(ReconhecedorFicticio::novo(Err(
            ErroDoReconhecimento::Arquivo("sumiu".to_owned()),
        ))));
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "a.pdf")], None);
        esperar(&mut j);
        assert!(matches!(
            &j.reconhecimento.cartoes[0].situacao,
            SituacaoDoCartao::Falhou(m) if m.contains("sumiu")
        ));
    }

    #[test]
    fn cancelar_na_espera_tira_da_fila() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = (arquivo(&dir, "a.pdf"), arquivo(&dir, "b.pdf"));
        j.soltar_arquivos(vec![a, b.clone()], None);
        // O segundo ainda espera (a thread do primeiro pode terminar a qualquer momento, mas
        // o tique só avança quando chamado).
        j.cancelar_reconhecimento(1);
        esperar(&mut j);
        assert_eq!(
            j.reconhecimento.cartoes[1].situacao,
            SituacaoDoCartao::Cancelado
        );
        assert!(!r.recebidos.lock().expect("trava").contains(&b));
    }
}
