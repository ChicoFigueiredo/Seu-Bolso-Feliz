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

pub use bancada_navegador::politica::{Bloqueio, Candidato, Politica};
use unicode_normalization::UnicodeNormalization as _;

/// O que o script da ponte instala no navegador (captura, descrição de elemento e página
/// simplificada). O texto-base é do motor (`bancada-navegador`); o marcador
/// [`MARCADOR_DA_LISTA`] é trocado pela lista em JSON.
pub use bancada_navegador::chrome::{MARCADOR_DA_LISTA, SCRIPT_DA_PONTE};

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
    "efetivar",
    "efetive",
    "efetivacao",
    "agendar",
    "agende",
    "agendamento",
    "antecipar",
    "antecipe",
    "antecipacao",
    "recarga",
];

/// Termos que só casam como palavra inteira (para não pegar `pixel`, `ted` em `protegido`).
pub const PALAVRA: &[&str] = &[
    "pix",
    "ted",
    "pay",
    "buy",
    "wire",
    "compre",
    "comprar",
    "sacar",
    "saque",
    "invest",
    "investimento",
    "investimentos",
    "aplicar",
    "aplique",
    "aplicacao",
    "aplicacoes",
    "autorizar",
    "autorize",
    "quitar",
    "quite",
    "emprestimo",
    "emprestimos",
    "financiamento",
    "financiamentos",
    "assinar",
    "assine",
];

/// Frases de **consulta** que contêm um termo da lista e mesmo assim passam: o comprovante de
/// um pagamento já feito e o extrato de uma aplicação, de um empréstimo ou de um
/// financiamento são documentos para baixar. A exceção só apaga a própria frase: o que sobrar
/// no campo ("pagar", o destino `/pagamento/confirmar`…) continua sendo conferido.
pub const EXCECOES: &[&str] = &[
    "comprovante de pagamento",
    "comprovantes de pagamento",
    "comprovante de pagamentos",
    "comprovantes de pagamentos",
    "extrato de pagamento",
    "extrato de pagamentos",
    "extrato de transferencia",
    "extrato de transferencias",
    "extrato de investimento",
    "extrato de investimentos",
    "extrato de aplicacao",
    "extrato de aplicacoes",
    "extrato de emprestimo",
    "extrato de emprestimos",
    "extrato de financiamento",
    "extrato de financiamentos",
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
    "debito automatico",
    "debito em conta automatico",
    "cadastrar debito",
];

/// O tamanho máximo de cada campo examinado (um cartão clicável com a página inteira dentro
/// não deve bloquear por uma palavra perdida no meio).
pub const LIMITE_DO_CAMPO: usize = 300;

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
    let mut p = palavras(valor);
    for excecao in EXCECOES {
        let alvo: Vec<&str> = excecao.split(' ').collect();
        let mut i = 0;
        while i + alvo.len() <= p.len() {
            if p[i..i + alvo.len()]
                .iter()
                .map(String::as_str)
                .eq(alvo.iter().copied())
            {
                p.drain(i..i + alvo.len());
            } else {
                i += 1;
            }
        }
    }
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
        "excecoes": EXCECOES,
        "limite": LIMITE_DO_CAMPO,
    })
    .to_string()
}

/// O script da ponte com a lista no lugar.
#[must_use]
pub fn script_da_ponte() -> String {
    SCRIPT_DA_PONTE.replace(MARCADOR_DA_LISTA, &lista_em_json())
}

/// A lista de bloqueio do SBF como [`Politica`] do navegador: o Chrome e o CDP perguntam a ela
/// o que não se clica, quais URLs não saem e qual ponte se instala na página.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListaDeBloqueio;

impl Politica for ListaDeBloqueio {
    fn avaliar(&self, candidato: &Candidato) -> Option<Bloqueio> {
        avaliar(candidato)
    }
    fn avaliar_url(&self, url: &str) -> Option<Bloqueio> {
        avaliar_url(url)
    }
    fn script_da_ponte(&self) -> String {
        script_da_ponte()
    }
}

/// O texto (normalizado: sem acento, minúsculas, por palavras) que marca o botão de "atualizar
/// conexão" do agregador (o Meu Pluggy). É um conjunto **fechado e exato**: "Atualizar dados
/// cadastrais" não está nele (e a lista de bloqueio o barra).
pub const ATUALIZAR_CONEXAO: &[&str] = &[
    "atualizar",
    "sincronizar",
    "atualizar conexao",
    "reconectar",
];

/// O texto do elemento é exatamente um dos de [`ATUALIZAR_CONEXAO`]?
#[must_use]
pub fn e_atualizar_conexao(candidato: &Candidato) -> Option<String> {
    candidato
        .campos
        .iter()
        .find(|(campo, _)| campo == "texto")
        .map(|(_, valor)| palavras(valor).join(" "))
        .filter(|t| ATUALIZAR_CONEXAO.contains(&t.as_str()))
}

/// Confere um elemento que o roteiro **escrito** vai clicar, segundo a classe do manifesto:
///
/// - `consulta`: a lista normal, e o botão de atualizar conexão **não** é liberado (barrado);
/// - `atualizar_conexao`: a lista normal, com uma exceção nomeada: o elemento cujo texto é
///   exatamente "atualizar", "sincronizar", "atualizar conexão" ou "reconectar" passa. Os
///   outros campos dele (rótulo, destino, nome) continuam sendo conferidos, e pagar,
///   transferir, contratar e alterar cadastro seguem barrados.
#[must_use]
pub fn avaliar_para_classe(
    candidato: &Candidato,
    classe: crate::manifesto::Classe,
) -> Option<Bloqueio> {
    let Some(texto) = e_atualizar_conexao(candidato) else {
        return avaliar(candidato);
    };
    match classe {
        crate::manifesto::Classe::Consulta => Some(Bloqueio {
            campo: "texto".to_owned(),
            termo: format!("{texto} (só a classe atualizar_conexao clica aqui)"),
        }),
        crate::manifesto::Classe::AtualizarConexao => {
            let mut sem_texto = candidato.clone();
            sem_texto.campos.retain(|(campo, _)| campo != "texto");
            avaliar(&sem_texto)
        }
    }
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
    fn novos_termos_de_dinheiro_saindo_sao_bloqueados() {
        for texto in [
            "Aplicar",
            "Aplicação",
            "Autorizar pagamento",
            "Autorizar",
            "Efetivar",
            "Agendar",
            "Agendamento",
            "Quitar",
            "Antecipar parcelas",
            "Antecipação de recebíveis",
            "Recarga de celular",
            "Empréstimo",
            "Financiamento",
            "Investimentos",
            "Contratar",
            "Assinar contrato",
            "Débito automático",
            "Cadastrar débito automático",
        ] {
            assert!(bloqueia("texto", texto), "deveria bloquear: {texto}");
        }
        assert!(bloqueia("destino", "/emprestimo/simular"));
        assert!(bloqueia("id", "btnAgendar"));
    }

    #[test]
    fn comprovante_e_extrato_de_consulta_passam_por_excecao_explicita() {
        for texto in [
            "Comprovante de pagamento",
            "Baixar comprovante de pagamento",
            "Comprovantes de Pagamentos",
            "Extrato de investimentos",
            "Extrato de empréstimo",
            "Extrato de financiamento",
            "Extrato de aplicações",
            "Extrato",
        ] {
            assert!(!bloqueia("texto", texto), "não deveria bloquear: {texto}");
        }
        assert!(!bloqueia("destino", "/comprovante-de-pagamento/123.pdf"));
        // o outro lado: a exceção só apaga a frase; o resto do campo continua conferido
        for texto in [
            "Comprovante de pagamento. Pagar agora",
            "Extrato de empréstimo: contratar",
            "Extrato de aplicações e aplicar",
            "Pagamento",
            "Confirmar pagamento",
            "Empréstimo",
        ] {
            assert!(bloqueia("texto", texto), "deveria bloquear: {texto}");
        }
        assert!(bloqueia(
            "destino",
            "/pagamento/comprovante-de-pagamento/confirmar"
        ));
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
        assert!(!v["excecoes"].as_array().unwrap().is_empty());
        assert!(!script_da_ponte().contains("__SBF_LISTA__"));
    }

    #[test]
    fn a_lista_do_sbf_e_a_politica_do_navegador() {
        use bancada_navegador::cdp::decidir_requisicao;
        let p = ListaDeBloqueio;
        // a mesma lista vale para a rede do CDP, qualquer método e tipo
        assert!(
            decidir_requisicao("https://b.example/api/pagar", "POST", "Fetch", None, &p).is_some()
        );
        assert!(
            decidir_requisicao("https://b.example/transferir", "GET", "Document", None, &p)
                .is_some()
        );
        assert!(
            decidir_requisicao("https://b.example/extrato", "GET", "Document", None, &p).is_none()
        );
        // e o script da ponte sai com a lista no lugar
        assert!(!p.script_da_ponte().contains(MARCADOR_DA_LISTA));
        assert!(p.avaliar(&Candidato::com(&[("texto", "Pagar")])).is_some());
    }
}
