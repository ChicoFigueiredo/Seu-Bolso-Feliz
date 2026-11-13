//! A leitura da janela com as duas fontes: o inventário (fatia 1) e o livro-caixa (fatia
//! 2, a cópia local da estação). Uma `Consulta` só para a janela, que encaminha cada
//! pedido para a fonte dele. **As duas nunca se somam**: cada número continua dizendo a
//! sua [`Fonte`].
//!
//! Regras de encaminhamento:
//! - áreas de Dinheiro, `painel_da_area`, números (`ItensDoTotal`, `por_ciclo`, relatório)
//!   com `Fonte::LivroCaixa` → livro-caixa; o resto → inventário;
//! - tipos que só existem no livro-caixa (movimentação, cartão, centro, dívida…) →
//!   livro-caixa;
//! - tipos das duas fontes (conta, fatura, fornecedor, ciclo, documento): a página junta
//!   os campos das duas (cada valor com a sua fonte), e as abas vão para a fonte do tipo
//!   da aba; entre dois tipos compartilhados, vale a fonte que conhece o item (o UUID é do
//!   livro-caixa; o nome, do inventário: grill Q17).
//!
//! Sem livro-caixa ligado (estação não pareada), os pedidos do livro-caixa respondem
//! `NaoSuportado` com [`SEM_LIVRO`], que a janela mostra como aviso, não como erro.

use sbf_inventario::consulta::{
    Area, Contagens, ErroDeConsulta, Filtro, Fonte, Item, Painel, PainelDeArea, PedidoDeTabela,
    Pergunta, Rastro, ResultadoDeConsulta, Tabela, TotalDoCiclo,
};
use sbf_inventario::{Consulta, Ref, Tipo};

/// O motivo mostrado quando o livro-caixa ainda não chegou a esta estação.
pub const SEM_LIVRO: &str =
    "O livro-caixa ainda não chegou a esta estação: pareie e sincronize na área Sincronização.";

/// O tipo só existe no livro-caixa?
pub fn so_do_livro(tipo: Tipo) -> bool {
    matches!(
        tipo,
        Tipo::Movimentacao
            | Tipo::Transferencia
            | Tipo::Instituicao
            | Tipo::Produto
            | Tipo::Cartao
            | Tipo::Categoria
            | Tipo::Tag
            | Tipo::Centro
            | Tipo::Compromisso
            | Tipo::Recorrente
            | Tipo::Divida
            | Tipo::Parcela
            | Tipo::Contrato
    )
}

/// O tipo existe nas duas fontes?
pub fn compartilhado(tipo: Tipo) -> bool {
    matches!(
        tipo,
        Tipo::Conta | Tipo::Fatura | Tipo::Fornecedor | Tipo::Ciclo | Tipo::Documento
    )
}

/// A leitura com as duas fontes.
pub struct ConsultaComposta {
    inventario: Box<dyn Consulta>,
    livro: Option<Box<dyn Consulta>>,
}

impl std::fmt::Debug for ConsultaComposta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsultaComposta")
            .field("livro", &self.livro.is_some())
            .finish_non_exhaustive()
    }
}

impl ConsultaComposta {
    pub fn nova(inventario: Box<dyn Consulta>, livro: Option<Box<dyn Consulta>>) -> Self {
        Self { inventario, livro }
    }

    /// Há livro-caixa ligado?
    pub fn tem_livro(&self) -> bool {
        self.livro.is_some()
    }

    fn livro(&self) -> ResultadoDeConsulta<&dyn Consulta> {
        self.livro
            .as_deref()
            .ok_or_else(|| ErroDeConsulta::NaoSuportado(SEM_LIVRO.to_owned()))
    }

    /// O livro-caixa conhece este item (e o inventário não)?
    fn e_do_livro(&self, r: &Ref) -> bool {
        if so_do_livro(r.tipo) {
            return true;
        }
        let Some(livro) = self.livro.as_deref() else {
            return false;
        };
        matches!(livro.item(r), Ok(Some(_))) && !matches!(self.inventario.item(r), Ok(Some(_)))
    }
}

impl Consulta for ConsultaComposta {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        match pedido {
            PedidoDeTabela::Area { area, .. } if area.e_do_livro_caixa() => {
                self.livro()?.tabela(pedido)
            }
            PedidoDeTabela::Area { .. } => self.inventario.tabela(pedido),
            PedidoDeTabela::Ligados { de, tipo } => {
                if so_do_livro(de.tipo) || so_do_livro(*tipo) || self.e_do_livro(de) {
                    self.livro()?.tabela(pedido)
                } else {
                    self.inventario.tabela(pedido)
                }
            }
            PedidoDeTabela::ItensDoTotal { fonte, .. } => match fonte {
                Fonte::LivroCaixa => self.livro()?.tabela(pedido),
                Fonte::Inventario => self.inventario.tabela(pedido),
            },
        }
    }

    fn tabela_paginada(
        &self,
        pedido: &PedidoDeTabela,
        inicio: usize,
        limite: usize,
    ) -> ResultadoDeConsulta<sbf_inventario::consulta::PaginaDeTabela> {
        // O inventário real pagina pelo SQL; o livro-caixa usa o padrão (corta a tabela).
        let do_livro = match pedido {
            PedidoDeTabela::Area { area, .. } => area.e_do_livro_caixa(),
            PedidoDeTabela::Ligados { de, tipo } => {
                so_do_livro(de.tipo) || so_do_livro(*tipo) || self.e_do_livro(de)
            }
            PedidoDeTabela::ItensDoTotal { fonte, .. } => *fonte == Fonte::LivroCaixa,
        };
        if do_livro {
            self.livro()?.tabela_paginada(pedido, inicio, limite)
        } else {
            self.inventario.tabela_paginada(pedido, inicio, limite)
        }
    }

    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        if so_do_livro(alvo.tipo) {
            return match self.livro.as_deref() {
                Some(l) => l.item(alvo),
                None => Err(ErroDeConsulta::NaoSuportado(SEM_LIVRO.to_owned())),
            };
        }
        let inv = self.inventario.item(alvo)?;
        if !compartilhado(alvo.tipo) {
            return Ok(inv);
        }
        let livro = match self.livro.as_deref() {
            Some(l) => l.item(alvo).unwrap_or(None),
            None => None,
        };
        Ok(match (livro, inv) {
            (Some(mut l), Some(i)) => {
                // Uma página só (Q17): os campos do livro-caixa primeiro, depois os do
                // inventário, cada valor com a sua fonte. O nome é o do inventário (é o
                // que a trilha mostrou até aqui), e a chave continua a do pedido.
                l.campos.extend(i.campos);
                l.alvo = alvo.clone();
                l.rotulo = i.rotulo;
                Some(l)
            }
            (Some(l), None) => Some(l),
            (None, i) => i,
        })
    }

    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        if self.e_do_livro(alvo) {
            self.livro()?.rastro(alvo)
        } else {
            self.inventario.rastro(alvo)
        }
    }

    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        match fonte {
            Fonte::LivroCaixa => self.livro()?.por_ciclo(alvo, fonte, ciclos),
            Fonte::Inventario => self.inventario.por_ciclo(alvo, fonte, ciclos),
        }
    }

    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        self.inventario.visao_geral()
    }

    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        match fonte {
            Fonte::LivroCaixa => self.livro()?.relatorio(pergunta, fonte, filtro),
            Fonte::Inventario => self.inventario.relatorio(pergunta, fonte, filtro),
        }
    }

    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        self.inventario.contagens()
    }

    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        self.inventario.texto(achado)
    }

    fn painel_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        if area.e_do_livro_caixa() {
            self.livro()?.painel_da_area(area, filtro)
        } else {
            self.inventario.painel_da_area(area, filtro)
        }
    }
}
