//! O laço do agente (spec 15, D11): fala com o provedor, executa as chamadas de ferramenta e
//! devolve o resultado, até o primeiro rascunho que passa no pré-teste, até o modelo responder
//! sem chamar nada, até o teto de 8 chamadas (imposto aqui, pela [`Sessao`]) ou até o usuário
//! mandar parar. Só a tentativa que passa vira versão; se nenhuma passa, o fim do pedido grava
//! uma só, a melhor ([`Sessao::encerrar`]).
//!
//! Nenhum texto de documento vai a log ou erro, e nenhuma chave passa por aqui.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sbf_ia::{
    ChamadaDeFerramenta, Consumo, ErroDeIa, Mensagem, NaturezaDoCusto, PedidoDeConversa,
    ProvedorDeConversa, Registro,
};
use sbf_mcp::backup::Backups;

use crate::contexto::Contexto;
use crate::ferramentas::{
    ChamadaFeita, EstadoDaSessao, RascunhoCriado, Sessao, definicoes, motivo_do_pedido,
    rastro_resumido,
};
use crate::prompt::{instrucoes, pedido_do_usuario, saida_de_ferramenta};
use crate::registro::{completar, registrar_antes};

/// Limite de tokens de cada resposta do modelo.
pub const MAX_TOKENS_DA_RESPOSTA: u32 = 3000;
pub use crate::registro::DESTINATARIO;

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
    /// O primeiro pedido da conversa: num refinamento, o `motivo` da versão é este + o de agora.
    primeiro_pedido: Option<String>,
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
            primeiro_pedido: None,
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
    /// Cada envio ao provedor grava antes a linha em `saida_ia` (`registro`, ADR-015). Se o
    /// registro não grava, nada é enviado.
    ///
    /// # Errors
    /// Falha do provedor (limite, rede, chave) ou do registro. O que já foi gravado fica.
    pub fn pedir(
        &mut self,
        provedor: &dyn ProvedorDeConversa,
        pedido: &str,
        cancelar: &AtomicBool,
        registro: &dyn Registro,
        observar: &mut dyn FnMut(Evento<'_>),
    ) -> Result<Relatorio, ErroDeIa> {
        let base = self.mensagens.len();
        let r = self.pedir_sem_limpar(provedor, pedido, cancelar, registro, observar);
        if r.is_err() {
            // Um pedido que falhou não deixa a conversa pela metade para o próximo.
            self.mensagens.truncate(base);
        }
        r
    }

    fn pedir_sem_limpar(
        &mut self,
        provedor: &dyn ProvedorDeConversa,
        pedido: &str,
        cancelar: &AtomicBool,
        registro: &dyn Registro,
        observar: &mut dyn FnMut(Evento<'_>),
    ) -> Result<Relatorio, ErroDeIa> {
        let inicio = Instant::now();
        let modelo = provedor.modelo();
        let primeiro = self.mensagens.is_empty();
        let motivo = motivo_do_pedido(
            self.primeiro_pedido.as_deref().filter(|_| !primeiro),
            pedido,
        );
        let mut sessao = Sessao::nova(self.ctx.clone(), &motivo, &modelo, self.backups.clone());
        if primeiro {
            self.primeiro_pedido = Some(pedido.trim().to_owned());
            // O rastro dos padrões do item já vai no primeiro prompt (D8): poupa uma chamada.
            let rastro = rastro_resumido(&self.ctx);
            self.mensagens.push(Mensagem::Sistema(instrucoes()));
            self.mensagens.push(Mensagem::Usuario(pedido_do_usuario(
                &self.ctx.tela,
                pedido,
                rastro.as_deref(),
            )));
        } else {
            self.mensagens.push(Mensagem::Usuario(format!(
                "NOVO PEDIDO DO USUÁRIO:\n{}",
                pedido.trim()
            )));
        }
        let ferramentas = definicoes();
        let mut consumo = Consumo::default();
        let mut ultimo_texto = String::new();
        let mut alguma_sem_custo = false;
        let parada = loop {
            if cancelar.load(Ordering::Acquire) {
                break Parada::Interrompida;
            }
            let req = PedidoDeConversa {
                mensagens: self.mensagens.clone(),
                ferramentas: ferramentas.clone(),
                max_tokens_saida: MAX_TOKENS_DA_RESPOSTA,
            };
            // ADR-015: a linha de `saida_ia` vai ANTES do envio; sem ela, nada sai.
            let linha = registrar_antes(
                registro,
                &provedor.nome(),
                &provedor.modelo(),
                bytes_do_pedido(&req),
            )?;
            let resposta = match provedor.conversar(&req, cancelar) {
                Ok(r) => r,
                Err(ErroDeIa::Interrompido) => break Parada::Interrompida,
                Err(e) => return Err(e),
            };
            completar(registro, linha, resposta.consumo);
            consumo = soma(consumo, resposta.consumo);
            alguma_sem_custo |= resposta.custo_ausente;
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
        // No máximo um rascunho por pedido: o que não passou só vira versão aqui, uma vez, com a
        // melhor tentativa (D11). Quem interrompeu não quer versão nenhuma.
        let mut aviso = None;
        let mut parada = parada;
        if matches!(parada, Parada::Respondeu | Parada::Limite) {
            aviso = sessao.encerrar().err();
            if sessao.passou() {
                parada = Parada::Passou;
            }
        }
        let estado = sessao.estado();
        let melhor = estado.melhor();
        let mut resposta = texto_final(parada, &ultimo_texto, &estado, melhor);
        if let Some(a) = aviso {
            resposta = format!("{resposta}\nNão consegui gravar o rascunho: {a}");
        }
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
            natureza: NaturezaDoCusto::do_pedido(provedor.natureza_do_custo(), alguma_sem_custo),
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
            // D11: depois de um rascunho passar, o resto da resposta é recusado sem executar.
            let saida = if sessao.passou() {
                sessao.recusar_depois_de_passar(&c.nome, &c.argumentos)
            } else {
                sessao.executar(&c.nome, &c.argumentos)
            };
            if let Some(feita) = sessao.chamadas.last() {
                observar(Evento::Chamada(feita));
            }
            self.mensagens.push(Mensagem::Ferramenta {
                id: c.id.clone(),
                nome: c.nome.clone(),
                conteudo: saida_de_ferramenta(&c.nome, &saida),
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
            "Parei após {} chamadas de ferramenta (o limite por pedido). Nenhuma tentativa passou no pré-teste; gravei uma só versão, a melhor, {}. Ela não passou: {}\n{}",
            sessao.maximo,
            sobre(r),
            r.pre_teste.motivo,
            r.pre_teste.linhas().join("\n")
        ),
        (Parada::Limite, None) => format!(
            "Parei após {} chamadas de ferramenta (o limite por pedido), sem nenhum rascunho. Peça de novo, mais específico.",
            sessao.maximo
        ),
        (Parada::Respondeu, Some(r)) if r.neste_pedido && !r.pre_teste.passou => {
            let fala = ultimo.trim();
            format!(
                "{}{}Nenhuma tentativa passou no pré-teste; gravei uma só versão, a melhor, {}. Ela não passou: {}\n{}",
                fala,
                if fala.is_empty() { "" } else { "\n\n" },
                sobre(r),
                r.pre_teste.motivo,
                r.pre_teste.linhas().join("\n")
            )
        }
        (Parada::Interrompida, _) => "Parei a pedido.".to_owned(),
        (_, _) if !ultimo.trim().is_empty() => ultimo.trim().to_owned(),
        (_, _) => "Sem resposta do modelo.".to_owned(),
    }
}
