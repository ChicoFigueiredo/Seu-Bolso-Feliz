//! O laço do agente (spec 15, D11): fala com o provedor, executa as chamadas de ferramenta e
//! devolve o resultado, até o primeiro rascunho que passa no pré-teste, até o modelo responder
//! sem chamar nada, até o teto de 8 chamadas (imposto aqui, pela [`Sessao`]) ou até o usuário
//! mandar parar.
//!
//! Nenhum texto de documento vai a log ou erro, e nenhuma chave passa por aqui.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sbf_ia::{
    ChamadaDeFerramenta, Consumo, ErroDeIa, Mensagem, NaturezaDoCusto, NivelDeConfianca,
    PedidoDeConversa, ProvedorDeConversa, Registro, Saida,
};
use sbf_mcp::backup::Backups;

use crate::contexto::Contexto;
use crate::ferramentas::{ChamadaFeita, EstadoDaSessao, RascunhoCriado, Sessao, definicoes};
use crate::prompt::{instrucoes, pedido_do_usuario};

/// Limite de tokens de cada resposta do modelo.
pub const MAX_TOKENS_DA_RESPOSTA: u32 = 3000;
/// O destinatário em `saida_ia` (ADR-015): todos os provedores recebem o texto completo (D13).
pub const DESTINATARIO: &str = "rita";

/// Por que o laço parou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parada {
    /// Um rascunho passou no pré-teste.
    Passou,
    /// O modelo respondeu sem chamar ferramenta.
    Respondeu,
    /// O teto de chamadas foi atingido.
    Limite,
    /// O usuário mandou parar.
    Interrompida,
}

/// O que acontece durante o pedido, para a tela e a CLI mostrarem ao vivo.
#[derive(Debug, Clone, PartialEq)]
pub enum Evento<'a> {
    /// O modelo disse algo (texto intermediário).
    Fala(&'a str),
    /// Uma chamada de ferramenta foi feita e já tem resultado.
    Chamada(&'a ChamadaFeita),
    /// Tokens e custo acumulados até aqui.
    Consumo(Consumo),
}

/// O resultado de um pedido.
#[derive(Debug, Clone)]
pub struct Relatorio {
    /// A resposta para o usuário (do modelo, ou montada aqui quando o laço parou por conta
    /// própria).
    pub resposta: String,
    pub parada: Parada,
    pub chamadas: Vec<ChamadaFeita>,
    pub rascunhos: Vec<RascunhoCriado>,
    /// O índice do melhor rascunho em `rascunhos`.
    pub melhor: Option<usize>,
    pub consumo: Consumo,
    pub natureza: NaturezaDoCusto,
    pub modelo: String,
    pub duracao: Duration,
}

impl Relatorio {
    pub fn melhor(&self) -> Option<&RascunhoCriado> {
        self.melhor.and_then(|i| self.rascunhos.get(i))
    }
}

/// A conversa de um item: as mensagens ficam enquanto a janela está aberta.
pub struct Rita {
    ctx: Contexto,
    mensagens: Vec<Mensagem>,
    backups: Backups,
}

impl std::fmt::Debug for Rita {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rita")
            .field("mensagens", &self.mensagens.len())
            .finish_non_exhaustive()
    }
}

fn soma(a: Consumo, b: Consumo) -> Consumo {
    Consumo {
        tokens_entrada: a.tokens_entrada + b.tokens_entrada,
        tokens_saida: a.tokens_saida + b.tokens_saida,
        custo_dezmilesimos_usd: a.custo_dezmilesimos_usd + b.custo_dezmilesimos_usd,
        nominal: a.nominal || b.nominal,
    }
}

fn bytes_do_pedido(p: &PedidoDeConversa) -> u64 {
    let mut n = 0;
    for m in &p.mensagens {
        n += match m {
            Mensagem::Sistema(t) | Mensagem::Usuario(t) => t.len(),
            Mensagem::Assistente { texto, chamadas } => {
                texto.as_ref().map_or(0, String::len)
                    + chamadas
                        .iter()
                        .map(|c| c.argumentos.to_string().len())
                        .sum::<usize>()
            }
            Mensagem::Ferramenta { conteudo, .. } => conteudo.len(),
        };
    }
    u64::try_from(n).unwrap_or(u64::MAX)
}

impl Rita {
    pub fn nova(ctx: Contexto) -> Self {
        let backups = Backups::novo(&ctx.tela.cofre);
        Self {
            ctx,
            mensagens: Vec::new(),
            backups,
        }
    }

    pub fn contexto(&self) -> &Contexto {
        &self.ctx
    }

    /// A tela mudou (área, filtro, lista) sem o item mudar: o próximo pedido vê a nova.
    pub fn trocar_tela(&mut self, tela: crate::contexto::DadosDaTela) {
        self.ctx.tela = tela;
    }

    /// Quantas mensagens a conversa tem (a janela mostra "nova conversa" quando zera).
    pub fn mensagens(&self) -> usize {
        self.mensagens.len()
    }

    /// Um pedido. Os pedidos seguintes continuam a conversa (refinar um rascunho).
    ///
    /// # Errors
    /// Falha do provedor (limite, rede, chave). O que já foi gravado fica.
    pub fn pedir(
        &mut self,
        provedor: &dyn ProvedorDeConversa,
        pedido: &str,
        cancelar: &AtomicBool,
        registro: Option<&dyn Registro>,
        observar: &mut dyn FnMut(Evento<'_>),
    ) -> Result<Relatorio, ErroDeIa> {
        let inicio = Instant::now();
        let modelo = provedor.modelo();
        let mut sessao = Sessao::nova(self.ctx.clone(), pedido, &modelo, self.backups.clone());
        if self.mensagens.is_empty() {
            self.mensagens.push(Mensagem::Sistema(instrucoes()));
            self.mensagens
                .push(Mensagem::Usuario(pedido_do_usuario(&self.ctx.tela, pedido)));
        } else {
            self.mensagens.push(Mensagem::Usuario(format!(
                "NOVO PEDIDO DO USUÁRIO:\n{}",
                pedido.trim()
            )));
        }
        let ferramentas = definicoes();
        let mut consumo = Consumo::default();
        let mut ultimo_texto = String::new();
        let parada = loop {
            if cancelar.load(Ordering::Acquire) {
                break Parada::Interrompida;
            }
            let req = PedidoDeConversa {
                mensagens: self.mensagens.clone(),
                ferramentas: ferramentas.clone(),
                max_tokens_saida: MAX_TOKENS_DA_RESPOSTA,
            };
            let resposta = match provedor.conversar(&req, cancelar) {
                Ok(r) => r,
                Err(ErroDeIa::Interrompido) => break Parada::Interrompida,
                Err(e) => return Err(e),
            };
            consumo = soma(consumo, resposta.consumo);
            if let Some(r) = registro {
                let _ = r.registrar_saida(&Saida {
                    destinatario: DESTINATARIO.to_owned(),
                    provedor: provedor.nome(),
                    modelo: resposta.modelo.clone(),
                    nivel: NivelDeConfianca::Completo,
                    bytes_enviados: bytes_do_pedido(&req),
                    consumo: resposta.consumo,
                    quando: sbf_ia::agora_rfc3339(),
                });
            }
            observar(Evento::Consumo(consumo));
            if let Some(t) = &resposta.texto {
                ultimo_texto.clone_from(t);
                observar(Evento::Fala(t));
            }
            if resposta.chamadas.is_empty() {
                if let Some(t) = resposta.texto {
                    self.mensagens.push(Mensagem::Assistente {
                        texto: Some(t),
                        chamadas: Vec::new(),
                    });
                }
                break Parada::Respondeu;
            }
            self.mensagens.push(Mensagem::Assistente {
                texto: resposta.texto.clone(),
                chamadas: resposta.chamadas.clone(),
            });
            self.executar_chamadas(&mut sessao, &resposta.chamadas, observar);
            if sessao.passou() {
                break Parada::Passou;
            }
            if sessao.estourou() {
                break Parada::Limite;
            }
        };
        let estado = sessao.estado();
        let melhor = estado.melhor();
        let resposta = texto_final(parada, &ultimo_texto, &estado, melhor);
        if parada != Parada::Respondeu {
            // A conversa segue coerente para o próximo pedido.
            self.mensagens.push(Mensagem::Assistente {
                texto: Some(resposta.clone()),
                chamadas: Vec::new(),
            });
        }
        Ok(Relatorio {
            resposta,
            parada,
            chamadas: estado.chamadas,
            rascunhos: estado.rascunhos,
            melhor,
            consumo,
            natureza: provedor.natureza_do_custo(),
            modelo,
            duracao: inicio.elapsed(),
        })
    }

    fn executar_chamadas(
        &mut self,
        sessao: &mut Sessao,
        chamadas: &[ChamadaDeFerramenta],
        observar: &mut dyn FnMut(Evento<'_>),
    ) {
        for c in chamadas {
            let saida = sessao.executar(&c.nome, &c.argumentos);
            if let Some(feita) = sessao.chamadas.last() {
                observar(Evento::Chamada(feita));
            }
            self.mensagens.push(Mensagem::Ferramenta {
                id: c.id.clone(),
                nome: c.nome.clone(),
                conteudo: saida,
            });
        }
    }
}

/// A resposta quando o laço parou por conta própria (o modelo não escreve de novo: poupa token).
pub(crate) fn texto_final(
    parada: Parada,
    ultimo: &str,
    sessao: &EstadoDaSessao,
    melhor: Option<usize>,
) -> String {
    let rascunho = melhor.and_then(|i| sessao.rascunhos.get(i));
    let sobre = |r: &RascunhoCriado| {
        format!(
            "`{}` (versão {}, rascunho, ainda não ativo)",
            r.regra, r.versao
        )
    };
    match (parada, rascunho) {
        (Parada::Passou, Some(r)) => format!(
            "Criei o rascunho {}. Ele passou no pré-teste:\n{}\nConfira e, se estiver bom, use Ativar.",
            sobre(r),
            r.pre_teste.linhas().join("\n")
        ),
        (Parada::Limite, Some(r)) => format!(
            "Parei após {} chamadas de ferramenta (o limite por pedido). O melhor rascunho até aqui é {}, mas não passou no pré-teste: {}\n{}",
            sessao.maximo,
            sobre(r),
            r.pre_teste.motivo,
            r.pre_teste.linhas().join("\n")
        ),
        (Parada::Limite, None) => format!(
            "Parei após {} chamadas de ferramenta (o limite por pedido), sem nenhum rascunho. Peça de novo, mais específico.",
            sessao.maximo
        ),
        (Parada::Interrompida, _) => "Parei a pedido.".to_owned(),
        (_, _) if !ultimo.trim().is_empty() => ultimo.trim().to_owned(),
        (_, _) => "Sem resposta do modelo.".to_owned(),
    }
}
