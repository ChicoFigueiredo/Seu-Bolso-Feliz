//! O que a triagem examina: um achado ou um registro.

use sbf_inventario::{Ref, Tipo};

/// A identidade de um item da fila.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Alvo {
    /// Pelo `content_hash`.
    Achado(String),
    /// Pelo `id`.
    Registro(i64),
}

impl Alvo {
    /// O `alvo_tipo` das tabelas (`achado` ou `registro`).
    pub fn tipo(&self) -> &'static str {
        match self {
            Alvo::Achado(_) => "achado",
            Alvo::Registro(_) => "registro",
        }
    }

    /// O `alvo_id` das tabelas.
    pub fn id(&self) -> String {
        match self {
            Alvo::Achado(h) => h.clone(),
            Alvo::Registro(i) => i.to_string(),
        }
    }

    /// Lê `alvo_tipo` e `alvo_id` de volta.
    pub fn de(tipo: &str, id: &str) -> Option<Alvo> {
        match tipo {
            "achado" => Some(Alvo::Achado(id.to_owned())),
            "registro" => id.parse().ok().map(Alvo::Registro),
            _ => None,
        }
    }

    /// A referência para o `Comandos`.
    pub fn referencia(&self) -> Ref {
        match self {
            Alvo::Achado(h) => Ref::nova(Tipo::Achado, h.clone()),
            Alvo::Registro(i) => Ref::nova(Tipo::Registro, i.to_string()),
        }
    }
}

/// Um achado pronto para entrar no prompt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDeAchado {
    pub content_hash: String,
    /// Os nomes de arquivo (ou assuntos de e-mail) das origens, no máximo três.
    pub nomes: Vec<String>,
    pub mime_type: Option<String>,
    /// O que o classificador automático achou. `None` no piloto, para não vazar a resposta.
    pub document_type: Option<String>,
    pub fornecedor: Option<String>,
    pub data_chave: Option<String>,
    pub valor_centavos: Option<i64>,
    /// O texto extraído, inteiro (a montagem trunca).
    pub texto: Option<String>,
}

/// Um registro pronto para entrar no prompt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDeRegistro {
    pub id: i64,
    pub tipo: String,
    pub data: String,
    pub valor_centavos: i64,
    pub descricao: Option<String>,
    pub fonte: String,
    pub fornecedor_bruto: Option<String>,
    /// Os campos automáticos atuais. `None` no piloto.
    pub fornecedor: Option<String>,
    pub categoria: Option<String>,
    pub conta: Option<String>,
    /// Um trecho do documento de origem, só quando o registro não tem descrição nem
    /// fornecedor.
    pub trecho: Option<String>,
    /// Registros que parecem o mesmo fato (mesmo valor, data próxima), para `duplicata_de`.
    pub parecidos: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Achado(ItemDeAchado),
    Registro(ItemDeRegistro),
}

impl Item {
    pub fn alvo(&self) -> Alvo {
        match self {
            Item::Achado(a) => Alvo::Achado(a.content_hash.clone()),
            Item::Registro(r) => Alvo::Registro(r.id),
        }
    }
}
