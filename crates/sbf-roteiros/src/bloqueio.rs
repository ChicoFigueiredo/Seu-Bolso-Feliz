//! A lista de bloqueio (RO-02): o que um roteiro nunca clica nem envia.
//!
//! Um roteiro só consulta e baixa. Esta lista barra elementos cujo texto visível, rótulo
//! (`aria-label`, `title`, `alt`), `value`, `name`, `id` ou URL de destino (`href`, `action`,
//! `formaction`) indique pagar, transferir, PIX, TED, contratar, investir, resgatar, sacar,
//! comprar ou alterar cadastro, em português e em inglês, com ou sem acento.
//!
//! A mesma lista roda em dois lugares, e os dois têm de concordar:
//! - no **navegador**, num script injetado que captura `click` e `submit` na fase de captura
//!   ([`SCRIPT_DA_PONTE`], que recebe a lista serializada por [`lista_em_json`]);
//! - no **executor**, que confere todo passo de clicar ou baixar antes de executá-lo.
//!
//! A comparação normaliza: tira acento, põe em minúsculas, separa `camelCase`, `_`, `-`, `/`
//! e `.` e compara por palavra. Por isso `btnPagarAgora`, `/pagamento/confirmar` e `P A G A R`
//! (não) são tratados com cuidado: o primeiro e o segundo casam; o último não, e é um limite
//! conhecido (UNVERIFIED contra páginas hostis).

use std::fmt;
use unicode_normalization::UnicodeNormalization as _;

/// O que o script da ponte instala no navegador (captura, descrição de elemento e página
/// simplificada). O marcador `__SBF_LISTA__` é trocado pela lista em JSON.
pub const SCRIPT_DA_PONTE: &str = include_str!("ponte.js");

/// Termos que casam como pedaço de uma palavra (`btnpagar`, `transferencia`).
pub const CONTEM: &[&str] = &[
    "pagar",
    "pague",
    "pagament",
    "pagto",
    "pgto",
    "transfer",
    "contratar",
    "contrate",
    "contratacao",
    "resgat",
    "investir",
    "invista",
    "payment",
    "withdraw",
    "redeem",
    "purchase",
    "checkout",
    "subscribe",
];

/// Termos que só casam como palavra inteira (para não pegar `pixel`, `ted` em `protegido`).
pub const PALAVRA: &[&str] = &[
    "pix", "ted", "pay", "buy", "wire", "compre", "comprar", "sacar", "saque", "invest",
];

/// Sequências de palavras que indicam alterar cadastro ou conta.
pub const FRASES: &[&str] = &[
    "alterar cadastro",
    "atualizar cadastro",
    "editar cadastro",
    "alterar dados",
    "atualizar dados",
    "alterar senha",
    "trocar senha",
    "alterar email",
    "alterar e mail",
    "alterar telefone",
    "excluir conta",
    "encerrar conta",
    "cancelar conta",
    "enviar dinheiro",
    "solicitar emprestimo",
    "change password",
    "change email",
    "update profile",
    "edit profile",
    "delete account",
    "close account",
    "send money",
];

/// O tamanho máximo de cada campo examinado (um cartão clicável com a página inteira dentro
/// não deve bloquear por uma palavra perdida no meio).
pub const LIMITE_DO_CAMPO: usize = 300;

/// Um elemento descrito para a conferência: cada campo é examinado sozinho.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidato {
    /// Pares (nome do campo, valor): `texto`, `aria-label`, `value`, `name`, `id`, `title`,
    /// `alt`, `destino`. Nunca leva o valor de um campo de senha.
    pub campos: Vec<(String, String)>,
    /// O elemento é um campo de senha (o executor não deixa preenchê-lo com texto literal).
    pub eh_senha: bool,
    /// O elemento é um `input`/`textarea`/`select` (para os passos de preencher).
    pub eh_campo: bool,
}

impl Candidato {
    /// Um candidato só com alguns campos, para testes e para o executor.
    #[must_use]
    pub fn com(campos: &[(&str, &str)]) -> Self {
        Self {
            campos: campos
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            ..Self::default()
        }
    }
}

/// Por que um elemento foi barrado. O `trecho` é só o termo e o campo, nunca o texto todo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloqueio {
    /// O campo que casou (`texto`, `destino`…).
    pub campo: String,
    /// O termo da lista que casou.
    pub termo: String,
}

impl fmt::Display for Bloqueio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "o campo \"{}\" do elemento indica \"{}\", e roteiro só consulta e baixa",
            self.campo, self.termo
        )
    }
}

/// Quebra `texto` em palavras minúsculas, sem acento, separando `camelCase`.
#[must_use]
pub fn palavras(texto: &str) -> Vec<String> {
    let mut separado = String::with_capacity(texto.len() + 8);
    let mut anterior: Option<char> = None;
    for c in texto.chars().take(LIMITE_DO_CAMPO) {
        if let Some(a) = anterior
            && a.is_lowercase()
            && c.is_uppercase()
        {
            separado.push(' ');
        }
        separado.push(c);
        anterior = Some(c);
    }
    let sem_acento: String = separado
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase();
    sem_acento
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// O termo da lista que casa com `valor`, se algum.
#[must_use]
pub fn termo_que_casa(valor: &str) -> Option<String> {
    let p = palavras(valor);
    for palavra in &p {
        if let Some(t) = CONTEM.iter().find(|t| palavra.contains(**t)) {
            return Some((*t).to_owned());
        }
        if let Some(t) = PALAVRA.iter().find(|t| palavra == *t) {
            return Some((*t).to_owned());
        }
    }
    for frase in FRASES {
        let alvo: Vec<&str> = frase.split(' ').collect();
        if p.windows(alvo.len())
            .any(|w| w.iter().map(String::as_str).eq(alvo.iter().copied()))
        {
            return Some((*frase).to_owned());
        }
    }
    None
}

/// Confere um elemento. `Some` se algum campo casa.
#[must_use]
pub fn avaliar(candidato: &Candidato) -> Option<Bloqueio> {
    candidato.campos.iter().find_map(|(campo, valor)| {
        termo_que_casa(valor).map(|termo| Bloqueio {
            campo: campo.clone(),
            termo,
        })
    })
}

/// Confere uma URL (o passo de navegar, ou o destino de um link).
#[must_use]
pub fn avaliar_url(url: &str) -> Option<Bloqueio> {
    avaliar(&Candidato::com(&[("destino", url)]))
}

/// A lista como JSON para o script da ponte: `{contem, palavra, frases}`.
#[must_use]
pub fn lista_em_json() -> String {
    serde_json::json!({
        "contem": CONTEM,
        "palavra": PALAVRA,
        "frases": FRASES,
        "limite": LIMITE_DO_CAMPO,
    })
    .to_string()
}

/// O script da ponte com a lista no lugar.
#[must_use]
pub fn script_da_ponte() -> String {
    SCRIPT_DA_PONTE.replace("__SBF_LISTA__", &lista_em_json())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bloqueia(campo: &str, valor: &str) -> bool {
        avaliar(&Candidato::com(&[(campo, valor)])).is_some()
    }

    #[test]
    fn botoes_perigosos_em_portugues_e_ingles() {
        for texto in [
            "Pagar",
            "PAGAR",
            "Confirmar pagamento",
            "Confirmar Pagamento agora",
            "Transferir",
            "Transferência",
            "TRANSFERENCIA",
            "PIX",
            "Pix copia e cola",
            "Pagar com Pix",
            "TED",
            "Contratar",
            "Contratação de seguro",
            "Resgatar",
            "Resgate total",
            "Investir",
            "Sacar",
            "Comprar",
            "Pay now",
            "Make a payment",
            "Wire transfer",
            "Withdraw",
            "Alterar cadastro",
            "Alterar Senha",
            "Atualizar dados cadastrais",
            "Change password",
            "Enviar dinheiro",
        ] {
            assert!(bloqueia("texto", texto), "deveria bloquear: {texto}");
        }
    }

    #[test]
    fn identificadores_e_destinos_disfarcados() {
        assert!(bloqueia("id", "btnPagarAgora"));
        assert!(bloqueia("id", "btn_pix"));
        assert!(bloqueia("name", "confirmar-pagamento"));
        assert!(bloqueia("name", "acaoTransferir"));
        assert!(bloqueia("value", "Confirmar pagamento"));
        assert!(bloqueia("aria-label", "Efetuar transferência"));
        assert!(bloqueia("destino", "/pagamento/confirmar"));
        assert!(bloqueia("destino", "https://x.example/api/pix?valor=1"));
        assert!(bloqueia("destino", "/transferencia"));
        assert!(bloqueia("destino", "/investimentos/resgatar"));
        assert!(bloqueia("id", "btnpagar"));
    }

    #[test]
    fn consulta_e_download_nao_sao_bloqueados() {
        for texto in [
            "Baixar extrato",
            "Extrato de março",
            "Ver extrato",
            "Entrar",
            "Continuar",
            "Informe de rendimentos",
            "Consultar saldo",
            "Documentos",
            "Pixel art",
            "Protegido",
            "Arquivo PDF",
            "Sair",
            "Enviar",
            "Buscar",
        ] {
            assert!(!bloqueia("texto", texto), "não deveria bloquear: {texto}");
        }
        assert!(!bloqueia("destino", "/extrato.pdf"));
        assert!(!bloqueia("id", "btnBaixar"));
    }

    #[test]
    fn acento_e_caixa_nao_enganam() {
        assert!(bloqueia("texto", "TRANSFERÊNCIA"));
        assert!(bloqueia("texto", "pagaménto"));
        assert!(bloqueia("texto", "Contratação"));
    }

    #[test]
    fn so_o_campo_conta_e_nao_a_juncao_de_campos() {
        // "alterar" num campo e "cadastro" noutro não formam a frase.
        let c = Candidato::com(&[("texto", "alterar"), ("name", "cadastro")]);
        assert!(avaliar(&c).is_none());
        let c = Candidato::com(&[("texto", "Alterar cadastro")]);
        let b = avaliar(&c).unwrap();
        assert_eq!(b.campo, "texto");
        assert_eq!(b.termo, "alterar cadastro");
    }

    #[test]
    fn o_motivo_nao_repete_o_texto_do_elemento() {
        let c = Candidato::com(&[("texto", "Pagar fatura de 1234,56 do cartão final 9999")]);
        let motivo = avaliar(&c).unwrap().to_string();
        assert!(!motivo.contains("1234"));
        assert!(!motivo.contains("9999"));
        assert!(motivo.contains("pagar"));
    }

    #[test]
    fn lista_em_json_leva_os_tres_grupos() {
        let v: serde_json::Value = serde_json::from_str(&lista_em_json()).unwrap();
        assert!(v["contem"].as_array().unwrap().len() > 5);
        assert!(v["palavra"].as_array().unwrap().iter().any(|t| t == "pix"));
        assert!(v["frases"].as_array().unwrap().len() > 5);
        assert!(!script_da_ponte().contains("__SBF_LISTA__"));
    }
}
