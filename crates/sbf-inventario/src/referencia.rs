//! Identidade de todo item do produto: `Ref { tipo, chave }`, escrita `tipo:chave`.
//!
//! Dono: integrador. É a costura entre a leitura (`consulta`), os comandos
//! (`comando`) e a janela. Mude só com acordo entre os três.
//!
//! Regra da spec 06 ("Navegação relacional"): toda entidade com identidade própria tem
//! página, e todo nome visível é link para ela. A lista de tipos não é fechada: entidade
//! nova nasce com um `Tipo` aqui.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// O tipo de um item. O texto estável (`codigo`) é o mesmo do protótipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tipo {
    // Dinheiro
    Movimentacao,
    Transferencia,
    Conta,
    Instituicao,
    Produto,
    Cartao,
    Fatura,
    Ciclo,
    Categoria,
    Tag,
    Centro,
    Compromisso,
    Recorrente,
    Divida,
    Parcela,
    Fornecedor,
    Contrato,
    // Documentos e inventário
    Documento,
    Achado,
    Origem,
    Registro,
    Decisao,
    Lote,
    Pendencia,
    RegraDeSenha,
    // Fontes
    Conexao,
    Coleta,
    Alerta,
    Canal,
    Roteiro,
    // Estação e análise
    Estacao,
    Alteracao,
    Ponto,
    Destinatario,
}

impl Tipo {
    /// Todos os tipos, na ordem do painel de navegação.
    pub const TODOS: [Tipo; 34] = [
        Tipo::Movimentacao,
        Tipo::Transferencia,
        Tipo::Conta,
        Tipo::Instituicao,
        Tipo::Produto,
        Tipo::Cartao,
        Tipo::Fatura,
        Tipo::Ciclo,
        Tipo::Categoria,
        Tipo::Tag,
        Tipo::Centro,
        Tipo::Compromisso,
        Tipo::Recorrente,
        Tipo::Divida,
        Tipo::Parcela,
        Tipo::Fornecedor,
        Tipo::Contrato,
        Tipo::Documento,
        Tipo::Achado,
        Tipo::Origem,
        Tipo::Registro,
        Tipo::Decisao,
        Tipo::Lote,
        Tipo::Pendencia,
        Tipo::RegraDeSenha,
        Tipo::Conexao,
        Tipo::Coleta,
        Tipo::Alerta,
        Tipo::Canal,
        Tipo::Roteiro,
        Tipo::Estacao,
        Tipo::Alteracao,
        Tipo::Ponto,
        Tipo::Destinatario,
    ];

    /// O texto estável do tipo, usado em `Ref` (`fornecedor:energia-exemplo`).
    pub fn codigo(self) -> &'static str {
        match self {
            Tipo::Movimentacao => "mov",
            Tipo::Transferencia => "transferencia",
            Tipo::Conta => "conta",
            Tipo::Instituicao => "instituicao",
            Tipo::Produto => "produto",
            Tipo::Cartao => "cartao",
            Tipo::Fatura => "fatura",
            Tipo::Ciclo => "ciclo",
            Tipo::Categoria => "categoria",
            Tipo::Tag => "tag",
            Tipo::Centro => "centro",
            Tipo::Compromisso => "compromisso",
            Tipo::Recorrente => "recorrente",
            Tipo::Divida => "divida",
            Tipo::Parcela => "parcela",
            Tipo::Fornecedor => "fornecedor",
            Tipo::Contrato => "contrato",
            Tipo::Documento => "documento",
            Tipo::Achado => "achado",
            Tipo::Origem => "origem",
            Tipo::Registro => "registro",
            Tipo::Decisao => "decisao",
            Tipo::Lote => "lote",
            Tipo::Pendencia => "pendencia",
            Tipo::RegraDeSenha => "regra-senha",
            Tipo::Conexao => "conexao",
            Tipo::Coleta => "coleta",
            Tipo::Alerta => "alerta",
            Tipo::Canal => "canal",
            Tipo::Roteiro => "roteiro",
            Tipo::Estacao => "estacao",
            Tipo::Alteracao => "alteracao",
            Tipo::Ponto => "ponto",
            Tipo::Destinatario => "destinatario",
        }
    }

    /// O inverso de [`Tipo::codigo`].
    pub fn de_codigo(codigo: &str) -> Option<Tipo> {
        Tipo::TODOS.into_iter().find(|t| t.codigo() == codigo)
    }

    /// Nome no singular, como aparece na trilha e no título da página.
    pub fn singular(self) -> &'static str {
        match self {
            Tipo::Movimentacao => "Movimentação",
            Tipo::Transferencia => "Transferência",
            Tipo::Conta => "Conta",
            Tipo::Instituicao => "Instituição",
            Tipo::Produto => "Produto financeiro",
            Tipo::Cartao => "Cartão",
            Tipo::Fatura => "Fatura",
            Tipo::Ciclo => "Ciclo",
            Tipo::Categoria => "Categoria",
            Tipo::Tag => "Tag",
            Tipo::Centro => "Centro",
            Tipo::Compromisso => "Compromisso",
            Tipo::Recorrente => "Recorrente",
            Tipo::Divida => "Dívida",
            Tipo::Parcela => "Parcela",
            Tipo::Fornecedor => "Fornecedor",
            Tipo::Contrato => "Contrato",
            Tipo::Documento => "Documento",
            Tipo::Achado => "Achado",
            Tipo::Origem => "Origem",
            Tipo::Registro => "Registro",
            Tipo::Decisao => "Decisão",
            Tipo::Lote => "Lote",
            Tipo::Pendencia => "Pendência",
            Tipo::RegraDeSenha => "Regra de senha",
            Tipo::Conexao => "Conexão",
            Tipo::Coleta => "Coleta",
            Tipo::Alerta => "Alerta",
            Tipo::Canal => "Canal",
            Tipo::Roteiro => "Roteiro",
            Tipo::Estacao => "Estação",
            Tipo::Alteracao => "Alteração",
            Tipo::Ponto => "Ponto de atenção",
            Tipo::Destinatario => "Destinatário",
        }
    }

    /// Nome no plural, como aparece no painel de navegação e na trilha.
    pub fn plural(self) -> &'static str {
        match self {
            Tipo::Movimentacao => "Movimentações",
            Tipo::Transferencia => "Transferências",
            Tipo::Conta => "Contas",
            Tipo::Instituicao => "Instituições",
            Tipo::Produto => "Produtos financeiros",
            Tipo::Cartao => "Cartões",
            Tipo::Fatura => "Faturas",
            Tipo::Ciclo => "Ciclos",
            Tipo::Categoria => "Categorias",
            Tipo::Tag => "Tags",
            Tipo::Centro => "Centros",
            Tipo::Compromisso => "Compromissos",
            Tipo::Recorrente => "Recorrentes",
            Tipo::Divida => "Dívidas",
            Tipo::Parcela => "Parcelas",
            Tipo::Fornecedor => "Fornecedores",
            Tipo::Contrato => "Contratos",
            Tipo::Documento => "Documentos",
            Tipo::Achado => "Achados",
            Tipo::Origem => "Origens",
            Tipo::Registro => "Registros",
            Tipo::Decisao => "Decisões",
            Tipo::Lote => "Lotes",
            Tipo::Pendencia => "Pendências",
            Tipo::RegraDeSenha => "Regras de senha",
            Tipo::Conexao => "Conexões",
            Tipo::Coleta => "Coletas",
            Tipo::Alerta => "Alertas",
            Tipo::Canal => "Canais",
            Tipo::Roteiro => "Roteiros",
            Tipo::Estacao => "Estações",
            Tipo::Alteracao => "Alterações",
            Tipo::Ponto => "Pontos de atenção",
            Tipo::Destinatario => "Destinatários",
        }
    }

    /// Nome do ícone em Material Symbols Rounded.
    pub fn icone(self) -> &'static str {
        match self {
            Tipo::Movimentacao => "swap_vert",
            Tipo::Transferencia => "sync_alt",
            Tipo::Conta => "account_balance_wallet",
            Tipo::Instituicao => "account_balance",
            Tipo::Produto => "savings",
            Tipo::Cartao => "credit_card",
            Tipo::Fatura => "receipt_long",
            Tipo::Ciclo => "date_range",
            Tipo::Categoria => "category",
            Tipo::Tag => "sell",
            Tipo::Centro => "hub",
            Tipo::Compromisso => "event",
            Tipo::Recorrente => "repeat",
            Tipo::Divida => "trending_down",
            Tipo::Parcela => "splitscreen",
            Tipo::Fornecedor => "storefront",
            Tipo::Contrato => "contract",
            Tipo::Documento => "description",
            Tipo::Achado => "find_in_page",
            Tipo::Origem => "folder_open",
            Tipo::Registro => "list_alt",
            Tipo::Decisao => "gavel",
            Tipo::Lote => "inventory_2",
            Tipo::Pendencia => "help",
            Tipo::RegraDeSenha => "key",
            Tipo::Conexao => "cable",
            Tipo::Coleta => "travel_explore",
            Tipo::Alerta => "notifications",
            Tipo::Canal => "forum",
            Tipo::Roteiro => "route",
            Tipo::Estacao => "computer",
            Tipo::Alteracao => "edit_note",
            Tipo::Ponto => "lightbulb",
            Tipo::Destinatario => "shield_person",
        }
    }

    /// A fatia em que a página deste tipo passa a funcionar com dados reais
    /// (spec 06, "Nesta fatia"). Antes disso a página aparece com o selo e o motivo.
    ///
    /// Na fatia 1, `documento:<hash>` e `achado:<hash>` são o mesmo conteúdo: o
    /// documento é o achado em estado Documento.
    pub fn fatia(self) -> u8 {
        match self {
            Tipo::Achado
            | Tipo::Documento
            | Tipo::Origem
            | Tipo::Registro
            | Tipo::Decisao
            | Tipo::Lote
            | Tipo::Pendencia
            | Tipo::RegraDeSenha
            | Tipo::Conexao
            | Tipo::Coleta
            | Tipo::Fornecedor
            | Tipo::Ciclo
            | Tipo::Conta
            | Tipo::Fatura => 1,
            Tipo::Alerta | Tipo::Canal => 4,
            Tipo::Roteiro => 5,
            Tipo::Ponto | Tipo::Destinatario => 6,
            _ => 2,
        }
    }

    /// O motivo mostrado numa página ou aba que ainda não funciona.
    pub fn motivo_da_fatia(self) -> Option<String> {
        match self.fatia() {
            1 => None,
            2 => Some("Chega na fatia 2, com o livro-caixa".to_owned()),
            4 => Some("Chega na fatia 4, com conexões, alertas e canais".to_owned()),
            5 => Some("Chega na fatia 5, com os roteiros".to_owned()),
            6 => Some("Chega na fatia 6, com a análise".to_owned()),
            n => Some(format!("Chega na fatia {n}")),
        }
    }
}

impl fmt::Display for Tipo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

/// A referência a um item: o tipo e a chave dele no banco.
///
/// Chaves da fatia 1: achado e documento = `content_hash`; origem, registro,
/// decisão, lote, pendência, regra de senha e coleta (`varredura`) = `id`;
/// conexão = `fonte.id` (pode ter `:`, como `disco:e2`); ciclo = `AAAA-MM`;
/// fornecedor e conta = o nome como está no banco. A chave da fatura é definida pela
/// vista `faturas_conferencia`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ref {
    pub tipo: Tipo,
    pub chave: String,
}

impl Ref {
    pub fn nova(tipo: Tipo, chave: impl Into<String>) -> Self {
        Self {
            tipo,
            chave: chave.into(),
        }
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.tipo.codigo(), self.chave)
    }
}

/// Erro ao ler uma `Ref` de texto.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("referência inválida: {0:?}")]
pub struct RefInvalida(pub String);

impl FromStr for Ref {
    type Err = RefInvalida;

    /// Separa no **primeiro** `:`, porque a chave pode ter `:` (`conexao:disco:e2`).
    fn from_str(texto: &str) -> Result<Self, Self::Err> {
        let (codigo, chave) = texto
            .split_once(':')
            .ok_or_else(|| RefInvalida(texto.to_owned()))?;
        let tipo = Tipo::de_codigo(codigo).ok_or_else(|| RefInvalida(texto.to_owned()))?;
        if chave.is_empty() {
            return Err(RefInvalida(texto.to_owned()));
        }
        Ok(Ref::nova(tipo, chave))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todo_tipo_ida_e_volta_pelo_codigo() {
        for tipo in Tipo::TODOS {
            assert_eq!(Tipo::de_codigo(tipo.codigo()), Some(tipo));
        }
    }

    #[test]
    fn ref_separa_no_primeiro_dois_pontos() {
        let r: Ref = "conexao:disco:e2".parse().expect("ref válida");
        assert_eq!(r, Ref::nova(Tipo::Conexao, "disco:e2"));
        assert_eq!(r.to_string(), "conexao:disco:e2");
    }

    #[test]
    fn ref_invalida() {
        assert!("semdoispontos".parse::<Ref>().is_err());
        assert!("naoexiste:1".parse::<Ref>().is_err());
        assert!("achado:".parse::<Ref>().is_err());
    }

    #[test]
    fn so_os_tipos_da_fatia_1_funcionam() {
        assert_eq!(Tipo::Fornecedor.motivo_da_fatia(), None);
        assert!(Tipo::Movimentacao.motivo_da_fatia().is_some());
    }
}
