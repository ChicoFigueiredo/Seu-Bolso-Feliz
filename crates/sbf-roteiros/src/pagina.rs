//! A página simplificada (RO-01) e a detecção de MFA e CAPTCHA (RO-03).
//!
//! A IA decide o próximo passo olhando esta versão da página: texto visível, elementos
//! clicáveis com seletor e formulários. **Não existe nenhum campo `valor` aqui**: os campos
//! só dizem se estão preenchidos. O que o navegador devolver além disso é ignorado por
//! construção (o `serde` descarta chave desconhecida), e [`Pagina::limpar`] ainda troca por
//! `***` qualquer segredo que a própria página tenha repetido no texto ou na URL.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Um elemento clicável.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elemento {
    pub seletor: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub rotulo: String,
    #[serde(default)]
    pub destino: String,
}

/// Um campo de formulário. Sem valor: só `preenchido`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campo {
    pub seletor: String,
    #[serde(default)]
    pub tipo: String,
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub rotulo: String,
    #[serde(default)]
    pub autocomplete: String,
    #[serde(default)]
    pub preenchido: bool,
}

/// Um formulário.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formulario {
    pub seletor: String,
    #[serde(default)]
    pub acao: String,
    #[serde(default)]
    pub metodo: String,
    #[serde(default)]
    pub campos: Vec<Campo>,
}

/// Uma tabela (o cabeçalho e o tamanho; o conteúdo só sai no passo de ler tabela).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tabela {
    pub seletor: String,
    #[serde(default)]
    pub linhas: usize,
    #[serde(default)]
    pub cabecalho: Vec<String>,
}

/// A página, como o executor, a detecção e a IA a enxergam.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagina {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub titulo: String,
    #[serde(default)]
    pub texto: String,
    #[serde(default)]
    pub elementos: Vec<Elemento>,
    #[serde(default)]
    pub formularios: Vec<Formulario>,
    #[serde(default)]
    pub tabelas: Vec<Tabela>,
    #[serde(default)]
    pub quadros: Vec<String>,
}

/// Por que o roteiro precisa do usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoDePausa {
    /// Código de verificação, token ou aprovação em outro aparelho.
    Mfa,
    /// "Não sou um robô".
    Captcha,
    /// Confirmação sensível de identidade.
    Confirmacao,
}

impl MotivoDePausa {
    /// A frase para a janela ("Sua vez no navegador: …").
    #[must_use]
    pub fn frase(self) -> &'static str {
        match self {
            Self::Mfa => "o site pediu o código de verificação (MFA)",
            Self::Captcha => "o site mostrou um CAPTCHA",
            Self::Confirmacao => "o site pediu uma confirmação de identidade",
        }
    }
}

const TRECHOS_DE_MFA: &[&str] = &[
    "codigo de verificacao",
    "codigo de seguranca",
    "codigo de autenticacao",
    "codigo enviado",
    "enviamos um codigo",
    "autenticacao em dois fatores",
    "autenticacao de dois fatores",
    "verificacao em duas etapas",
    "verificacao em dois passos",
    "segundo fator",
    "informe o token",
    "digite o token",
    "aprove no aplicativo",
    "aprovar no aplicativo",
    "verification code",
    "two factor",
    "two-factor",
    "2fa",
    "one-time code",
    "one time code",
    "security code",
];
const TRECHOS_DE_CAPTCHA: &[&str] = &[
    "captcha",
    "nao sou um robo",
    "nao sou robo",
    "i am not a robot",
    "not a robot",
    "verify you are human",
    "verifique que voce e humano",
    "confirme que voce e humano",
];
const TRECHOS_DE_CONFIRMACAO: &[&str] = &[
    "confirme sua identidade",
    "confirmar sua identidade",
    "confirmacao de identidade",
    "verify your identity",
    "confirm your identity",
/// `trecho` aparece em `texto` como palavra (ou frase) inteira? "2fa" não casa dentro de
/// "x2fab" nem de um hash. Os dois já vêm em minúsculas e sem acento.
fn contem_trecho(texto: &str, trecho: &str) -> bool {
    let mut inicio = 0;
    while let Some(p) = texto[inicio..].find(trecho) {
        let a = inicio + p;
        let b = a + trecho.len();
        let antes = texto[..a].chars().next_back();
        let depois = texto[b..].chars().next();
        if !antes.is_some_and(char::is_alphanumeric) && !depois.is_some_and(char::is_alphanumeric) {
            return true;
        }
        inicio = a + trecho.chars().next().map_or(1, char::len_utf8);
    }
    false
}

];

fn sem_acento(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization as _;
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

impl Pagina {
    /// Aplica `f` a todo texto da página.
    fn mapear(&mut self, f: &dyn Fn(&str) -> String) {
        self.url = f(&self.url);
        self.titulo = f(&self.titulo);
        self.texto = f(&self.texto);
        for e in &mut self.elementos {
            e.texto = f(&e.texto);
            e.rotulo = f(&e.rotulo);
            e.destino = f(&e.destino);
        }
        for fo in &mut self.formularios {
            fo.acao = f(&fo.acao);
            for c in &mut fo.campos {
                c.rotulo = f(&c.rotulo);
                c.nome = f(&c.nome);
            }
        }
        for t in &mut self.tabelas {
            for c in &mut t.cabecalho {
                *c = f(c);
            }
        }
        for q in &mut self.quadros {
            *q = f(q);
        }
    }

    /// Troca por `***` toda ocorrência dos `segredos` (e da forma com `%`) em qualquer texto.
    /// Segredo com menos de 4 caracteres não é trocado (apagaria o texto todo).
    #[must_use]
    pub fn limpar(mut self, segredos: &[&str]) -> Self {
        if variantes_do_segredo(segredos).is_empty() {
            return self;
        }
        self.mapear(&|t| limpar_texto_com(t, segredos));
        self
    }

    /// Mascara CPF e sequências longas de dígitos (conta, cartão, linha digitável): o nível
    /// "mascarado" do destinatário (ADR-015).
    #[must_use]
    pub fn mascarar(mut self) -> Self {
        self.mapear(&mascarar_numeros);
        self
    }

    /// Há CAPTCHA, MFA ou pedido de confirmação na página?
    #[must_use]
    pub fn pausa_necessaria(&self) -> Option<MotivoDePausa> {
        let texto = sem_acento(&format!("{} {}", self.titulo, self.texto));
        let quadros = sem_acento(&self.quadros.join(" "));
        if quadros.contains("recaptcha")
            || quadros.contains("hcaptcha")
            || quadros.contains("h-captcha")
            || quadros.contains("turnstile")
            || quadros.contains("captcha")
            || TRECHOS_DE_CAPTCHA.iter().any(|t| contem_trecho(&texto, t))
        {
            return Some(MotivoDePausa::Captcha);
        }
        let campo_de_codigo = self
            .formularios
            .iter()
            .flat_map(|f| &f.campos)
            .any(|c| c.autocomplete.eq_ignore_ascii_case("one-time-code"));
        if campo_de_codigo || TRECHOS_DE_MFA.iter().any(|t| contem_trecho(&texto, t)) {
            return Some(MotivoDePausa::Mfa);
        }
        if TRECHOS_DE_CONFIRMACAO
            .iter()
            .any(|t| contem_trecho(&texto, t))
        {
            return Some(MotivoDePausa::Confirmacao);
        }
        None
    }

    /// A página como texto compacto para o prompt. Sem valores de campo.
    #[must_use]
    pub fn para_texto(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(s, "URL: {}", self.url);
        let _ = writeln!(s, "Título: {}", self.titulo);
        let texto: String = self.texto.chars().take(4000).collect();
        let _ = writeln!(s, "Texto visível: {texto}");
        let _ = writeln!(s, "Clicáveis:");
        for e in &self.elementos {
            let _ = writeln!(
                s,
                "- [{}] <{}> \"{}\" {} {}",
                e.seletor, e.tag, e.texto, e.rotulo, e.destino
            );
        }
        let _ = writeln!(s, "Formulários:");
        for f in &self.formularios {
            let _ = writeln!(s, "- [{}] {} {}", f.seletor, f.metodo, f.acao);
            for c in &f.campos {
                let _ = writeln!(
                    s,
                    "  - [{}] tipo={} nome={} rótulo=\"{}\" {}",
                    c.seletor,
                    c.tipo,
                    c.nome,
                    c.rotulo,
                    if c.preenchido {
                        "(preenchido)"
                    } else {
                        "(vazio)"
                    }
                );
            }
        }
        let _ = writeln!(s, "Tabelas:");
        for t in &self.tabelas {
            let _ = writeln!(
                s,
                "- [{}] {} linhas: {}",
                t.seletor,
                t.linhas,
                t.cabecalho.join(" | ")
            );
        }
        s
    }
}

/// As formas em que um segredo pode reaparecer: ele mesmo, percent-encoded e, se é um número
/// com pontuação (CPF), só os dígitos. Segredo com menos de 4 caracteres fica de fora.
fn variantes_do_segredo(segredos: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in segredos.iter().filter(|s| s.chars().count() >= 4) {
        v.push((*s).to_owned());
        v.push(codificar_url(s));
        let so_digitos: String = s.chars().filter(char::is_ascii_digit).collect();
        let numerico = s
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '/' | ' '));
        if numerico && so_digitos.len() >= 8 {
            v.push(so_digitos);
        }
    }
    v.sort_by_key(|x| std::cmp::Reverse(x.len()));
    v.dedup();
    v
}

/// Troca por `***` o segredo, a forma percent-encoded e (para um número com pontuação) só os
/// dígitos, em qualquer parte de `texto`.
#[must_use]
pub fn limpar_texto_com(texto: &str, segredos: &[&str]) -> String {
    let mut saida = texto.to_owned();
    for v in variantes_do_segredo(segredos) {
        if saida.contains(v.as_str()) {
            saida = saida.replace(v.as_str(), "***");
        }
    }
    saida
}

fn codificar_url(s: &str) -> String {
    let mut saida = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            saida.push(b as char);
        } else {
            saida.push_str(&format!("%{b:02X}"));
        }
    }
    saida
}

fn mascarar_numeros(t: &str) -> String {
    static CPF: OnceLock<Regex> = OnceLock::new();
    static LONGO: OnceLock<Regex> = OnceLock::new();
    static LINHA: OnceLock<Regex> = OnceLock::new();
    let cpf = CPF.get_or_init(|| Regex::new(r"\d{3}\.\d{3}\.\d{3}-\d{2}").expect("regex fixa"));
    let longo = LONGO.get_or_init(|| Regex::new(r"\d{8,}").expect("regex fixa"));
    let linha = LINHA.get_or_init(|| Regex::new(r"\d[\d. ]{18,}\d").expect("regex fixa"));
    let a = cpf.replace_all(t, "***.***.***-**");
    let b = linha.replace_all(&a, |c: &regex::Captures<'_>| {
        if c[0].chars().filter(char::is_ascii_digit).count() >= 18 {
            "[número longo]".to_owned()
        } else {
            c[0].to_owned()
        }
    });
    longo.replace_all(&b, "[número longo]").into_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    const SENHA: &str = "SenhaFicticia#4821";

    fn bruta_com_valores() -> serde_json::Value {
        serde_json::json!({
            "url": "http://127.0.0.1:1/login",
            "titulo": "Entrar",
            "texto": "Bem-vindo. Digite seus dados.",
            "elementos": [{"seletor": "#entrar", "tag": "button", "texto": "Entrar", "valor": SENHA}],
            "formularios": [{
                "seletor": "#f", "acao": "/login", "metodo": "post",
                "campos": [
                    {"seletor": "#cpf", "tipo": "text", "nome": "cpf", "preenchido": true, "valor": "123.456.789-09"},
                    {"seletor": "#senha", "tipo": "password", "nome": "senha", "preenchido": true,
                     "valor": SENHA, "value": SENHA}
                ]
            }]
        })
    }

    #[test]
    fn valores_do_navegador_nao_sobrevivem_a_pagina_simplificada() {
        let p: Pagina = serde_json::from_value(bruta_com_valores()).unwrap();
        let saida = serde_json::to_string(&p).unwrap() + &p.para_texto();
        assert!(!saida.contains(SENHA));
        assert!(!saida.contains("123.456.789-09"));
        assert!(saida.contains("(preenchido)"));
    }

    #[test]
    fn limpar_troca_o_segredo_que_a_pagina_repetiu() {
        let mut p: Pagina = serde_json::from_value(bruta_com_valores()).unwrap();
        p.texto = format!("Sua senha é {SENHA}!");
        p.url = format!("http://x/?s={}", codificar_url(SENHA));
        p.elementos[0].texto = SENHA.to_owned();
        let p = p.limpar(&[SENHA]);
        let saida = serde_json::to_string(&p).unwrap();
        assert!(!saida.contains(SENHA));
        assert!(!saida.contains("%23"), "forma codificada também some");
        assert!(saida.contains("***"));
    }

    #[test]
    fn mascarar_cobre_cpf_e_numero_longo() {
        let p = Pagina {
            texto: "CPF 123.456.789-09 conta 12345678901234 valor R$ 1.234,56".into(),
            ..Pagina::default()
        };
        let p = p.mascarar();
        assert!(!p.texto.contains("789-09"));
        assert!(!p.texto.contains("12345678901234"));
        assert!(p.texto.contains("1.234,56"));
    }

    fn com_texto(t: &str) -> Pagina {
        Pagina {
            texto: t.into(),
            ..Pagina::default()
        }
    }

    #[test]
    fn detecta_mfa_captcha_e_confirmacao() {
        assert_eq!(
            com_texto("Digite o Código de verificação enviado por SMS").pausa_necessaria(),
            Some(MotivoDePausa::Mfa)
        );
        assert_eq!(
            com_texto("Autenticação em dois fatores").pausa_necessaria(),
            Some(MotivoDePausa::Mfa)
        );
        assert_eq!(
            com_texto("Marque: Não sou um robô").pausa_necessaria(),
            Some(MotivoDePausa::Captcha)
        );
        let mut p = Pagina::default();
        p.quadros
            .push("https://www.google.com/recaptcha/api2/anchor".into());
        assert_eq!(p.pausa_necessaria(), Some(MotivoDePausa::Captcha));
        let mut p = Pagina::default();
        p.formularios.push(Formulario {
            campos: vec![Campo {
                autocomplete: "one-time-code".into(),
                ..Campo::default()
            }],
            ..Formulario::default()
        });
        assert_eq!(p.pausa_necessaria(), Some(MotivoDePausa::Mfa));
        assert_eq!(
            com_texto("Confirme sua identidade para continuar").pausa_necessaria(),
            Some(MotivoDePausa::Confirmacao)
        );
        assert_eq!(
            com_texto("Extratos do mês. Baixar PDF").pausa_necessaria(),
            None
        );
    }
}
    #[test]
    fn mfa_so_casa_palavra_inteira() {
        for t in [
            "Ative a 2FA da conta",
            "Use o 2FA.",
            "(2FA) obrigatório",
            "Two-factor authentication",
        ] {
            assert_eq!(
                com_texto(t).pausa_necessaria(),
                Some(MotivoDePausa::Mfa),
                "{t}"
            );
        }
        for t in [
            "classe x2fab e hash a1b2fa9c",
            "abc2fa",
            "2faz",
            "token3 security codex",
            "Extratos do mês. Baixar PDF",
        ] {
            assert_eq!(com_texto(t).pausa_necessaria(), None, "{t}");
        }
    }

