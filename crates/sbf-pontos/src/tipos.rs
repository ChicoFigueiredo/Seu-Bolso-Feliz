//! Os tipos de um ponto de atenção.

use serde::{Deserialize, Serialize};

/// A área do ponto (o filtro da janela).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Area {
    Dinheiro,
    Operacao,
    CustoDeIa,
    TempoDoUsuario,
}

impl Area {
    pub const TODAS: [Area; 4] = [
        Area::Dinheiro,
        Area::Operacao,
        Area::CustoDeIa,
        Area::TempoDoUsuario,
    ];

    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Area::Dinheiro => "dinheiro",
            Area::Operacao => "operacao",
            Area::CustoDeIa => "custo-ia",
            Area::TempoDoUsuario => "tempo",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Area> {
        Area::TODAS.into_iter().find(|a| a.codigo() == c)
    }

    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Area::Dinheiro => "Dinheiro",
            Area::Operacao => "Operação",
            Area::CustoDeIa => "Custo de IA",
            Area::TempoDoUsuario => "Seu tempo",
        }
    }
}

/// O estado de um ponto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    Aberto,
    Resolvido,
    Ignorado,
}

impl Estado {
    #[must_use]
    pub fn codigo(self) -> &'static str {
        match self {
            Estado::Aberto => "aberto",
            Estado::Resolvido => "resolvido",
            Estado::Ignorado => "ignorado",
        }
    }

    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Estado> {
        [Estado::Aberto, Estado::Resolvido, Estado::Ignorado]
            .into_iter()
            .find(|e| e.codigo() == c)
    }
}

/// Em que unidade vem o valor do impacto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unidade {
    /// Centavos de real.
    CentavosBrl,
    /// Centavos de dólar (custo de IA).
    CentavosUsd,
    /// Minutos do usuário.
    Minutos,
}

/// O impacto de um ponto. `estimativa` é `true` quando o valor é uma projeção (por exemplo,
/// o reajuste vezes doze): a janela mostra "estimativa" ao lado (PA-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impacto {
    pub valor: i64,
    pub unidade: Unidade,
    /// `mes`, `ano`, `ciclo`, `semana` ou `unico`.
    pub periodo: String,
    pub estimativa: bool,
}

fn reais(centavos: i64, simbolo: &str) -> String {
    let abs = centavos.unsigned_abs();
    let milhar = {
        let inteiro = (abs / 100).to_string();
        let mut s = String::new();
        for (i, c) in inteiro.chars().enumerate() {
            if i > 0 && (inteiro.len() - i).is_multiple_of(3) {
                s.push('.');
            }
            s.push(c);
        }
        s
    };
    format!("{simbolo} {milhar},{:02}", abs % 100)
}

impl Impacto {
    /// O texto sem o prefixo de estimativa: `R$ 144,00 por ano`. A janela mostra o selo
    /// "estimativa" ao lado.
    #[must_use]
    pub fn valor_texto(&self) -> String {
        let valor = match self.unidade {
            Unidade::CentavosBrl => reais(self.valor, "R$"),
            Unidade::CentavosUsd => reais(self.valor, "US$"),
            Unidade::Minutos => format!("{} min", self.valor),
        };
        let periodo = match self.periodo.as_str() {
            "mes" => " por mês",
            "ano" => " por ano",
            "ciclo" => " no ciclo",
            "semana" => " na semana",
            _ => "",
        };
        format!("{valor}{periodo}")
    }

    /// O texto de canais (Telegram): `estimativa: R$ 144,00 por ano` ou `R$ 45,00 no ciclo`.
    #[must_use]
    pub fn texto(&self) -> String {
        if self.estimativa {
            format!("estimativa: {}", self.valor_texto())
        } else {
            self.valor_texto()
        }
    }
}

/// Um item que prova o ponto, com o tipo e o id que a janela linka.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidencia {
    /// `achado`, `registro`, `movimentacao`, `fornecedor`, `fonte`, `saida_ia`…
    pub tipo: String,
    pub id: String,
    pub rotulo: String,
}

impl Evidencia {
    #[must_use]
    pub fn nova(tipo: &str, id: impl Into<String>, rotulo: impl Into<String>) -> Self {
        Self {
            tipo: tipo.to_owned(),
            id: id.into(),
            rotulo: rotulo.into(),
        }
    }
}

/// O que uma regra produz. `regra` + `chave` identificam o ponto (PA-02); `assinatura`
/// resume a evidência: se ela mudar, um ponto resolvido ou ignorado volta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    pub regra: &'static str,
    pub chave: String,
    pub area: Area,
    pub titulo: String,
    pub detalhe: String,
    pub evidencia: Vec<Evidencia>,
    pub impacto: Option<Impacto>,
    pub assinatura: String,
}

/// Um ponto guardado em `pontos.sqlite`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ponto {
    pub id: i64,
    pub regra: String,
    pub chave: String,
    pub area: Area,
    pub titulo: String,
    pub detalhe: String,
    pub evidencia: Vec<Evidencia>,
    pub impacto: Option<Impacto>,
    pub estado: Estado,
    pub motivo: Option<String>,
    pub criado_em: String,
    pub atualizado_em: String,
    /// Quando foi avisado por um canal (Telegram); `None` até lá.
    pub notificado_em: Option<String>,
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn impacto_estimado_vem_marcado() {
        let i = Impacto {
            valor: 1_234_500,
            unidade: Unidade::CentavosBrl,
            periodo: "ano".into(),
            estimativa: true,
        };
        assert_eq!(i.texto(), "estimativa: R$ 12.345,00 por ano");
        let j = Impacto {
            valor: 4500,
            unidade: Unidade::CentavosBrl,
            periodo: "ciclo".into(),
            estimativa: false,
        };
        assert_eq!(j.texto(), "R$ 45,00 no ciclo");
    }

    #[test]
    fn codigos_ida_e_volta() {
        for a in Area::TODAS {
            assert_eq!(Area::de_codigo(a.codigo()), Some(a));
        }
        assert_eq!(Estado::de_codigo("ignorado"), Some(Estado::Ignorado));
    }
}
