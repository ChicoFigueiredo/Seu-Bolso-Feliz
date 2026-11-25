//! O manifesto do **roteiro escrito**: o JSON que vem ao lado do `.luau` e diz ao que o roteiro
//! tem direito (hosts, rótulos do chaveiro, MFA, classe e agenda). O fonte Luau nunca decide isso.
//!
//! As regras duras ficam em [`Manifesto::validar`]; o `importar` recusa o que não passa. As
//! mensagens de erro nunca repetem rótulo de chaveiro, CPF nem código.
//!
//! ```json
//! {
//!   "nome": "contracheque-exemplo",
//!   "descricao": "texto",
//!   "url_inicial": "https://portal.exemplo.test/entrar",
//!   "hosts": ["portal.exemplo.test"],
//!   "rotulo_senha": "senha:portal-exemplo",
//!   "rotulo_cpf": null,
//!   "mfa": { "tipo": "email", "remetente": "nao-responda@portal.exemplo.test", "formato": "\\d{6}" },
//!   "movimenta_dinheiro": false,
//!   "classe": "consulta",
//!   "agenda_horas": 168
//! }
//! ```

use crate::erro::{ErroDeRoteiro, Resultado};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// O maior manifesto ou fonte aceito: 64 KiB.
pub const LIMITE_DE_BYTES: usize = 64 * 1024;

/// O caractere muda a direção do texto ou separa linhas por Unicode (U+202A..202E, U+2066..2069,
/// U+200E/F, U+061C, U+2028/9)? É o truque de mostrar no terminal um texto diferente do que o
/// arquivo guarda.
#[must_use]
pub fn e_bidi(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{061C}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// O texto tem caractere de controle (ESC, `\r`, `\n`, byte nulo, C1…) ou de direção? Com
/// `permite_quebra`, `\n` e `\t` não contam (o fonte do roteiro).
#[must_use]
pub fn tem_caractere_perigoso(texto: &str, permite_quebra: bool) -> bool {
    texto
        .chars()
        .any(|c| (c.is_control() && !(permite_quebra && matches!(c, '\n' | '\t'))) || e_bidi(c))
}

fn escapar_com(texto: &str, manter_quebra: bool) -> String {
    let mut s = String::with_capacity(texto.len());
    for c in texto.chars() {
        if manter_quebra && matches!(c, '\n' | '\t') {
            s.push(c);
        } else if c.is_control() || e_bidi(c) {
            let n = u32::from(c);
            if n <= 0xFF {
                s.push_str(&format!("\\x{n:02x}"));
            } else {
                s.push_str(&format!("\\u{{{n:04x}}}"));
            }
        } else {
            s.push(c);
        }
    }
    s
}

/// Troca controle e direção por um escape visível (`\x1b`, `\u{202e}`): **toda** string que
/// veio do banco (autor, motivo, descrição, rótulos, hosts…) passa por aqui antes de ir ao
/// terminal, para que ela nunca mova o cursor, apague linhas nem inverta o texto.
#[must_use]
pub fn escapar_para_terminal(texto: &str) -> String {
    escapar_com(texto, false)
}

/// Como [`escapar_para_terminal`], mas deixa `\n` e `\t` (o fonte do roteiro, linha a linha).
#[must_use]
pub fn escapar_fonte_para_terminal(texto: &str) -> String {
    escapar_com(texto, true)
}

/// Como o roteiro passa pela segunda etapa de autenticação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mfa {
    /// O site não pede.
    Nenhum,
    /// O usuário resolve no navegador (o código do aplicativo autenticador, o TOTP, que muda
    /// por minuto, ou um CAPTCHA): o roteiro pausa.
    Manual,
    /// O código chega por e-mail e o motor o busca no Gmail (só para quem não movimenta dinheiro).
    Email {
        /// O endereço fixo que envia o código.
        remetente: String,
        /// A expressão do código (só dígitos e letras, tamanho fixo), por exemplo `\d{6}`.
        formato: String,
    },
}

/// O que o roteiro pode fazer nas conexões do Pluggy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classe {
    /// Só consulta e baixa (o padrão).
    #[default]
    Consulta,
    /// Também libera clicar em "Atualizar" ou "Sincronizar" nas conexões. Nunca pagar,
    /// transferir, contratar ou alterar cadastro: a lista de bloqueio continua valendo.
    AtualizarConexao,
}

/// O manifesto de um roteiro escrito.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifesto {
    pub nome: String,
    #[serde(default)]
    pub descricao: String,
    pub url_inicial: String,
    /// A allowlist: o roteiro não navega nem digita segredo fora destes hosts.
    pub hosts: Vec<String>,
    #[serde(default)]
    pub rotulo_senha: Option<String>,
    #[serde(default)]
    pub rotulo_cpf: Option<String>,
    pub mfa: Mfa,
    /// Obrigatório de propósito: o esquecimento não pode valer "não movimenta".
    pub movimenta_dinheiro: bool,
    #[serde(default)]
    pub classe: Classe,
    #[serde(default)]
    pub agenda_horas: Option<u32>,
}

fn invalido(m: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::Invalido(format!("manifesto: {}", m.into()))
}

impl Manifesto {
    /// Lê o JSON e valida (as duas coisas juntas, para nunca existir manifesto lido e não
    /// validado).
    ///
    /// # Errors
    /// JSON inválido, campo desconhecido ou regra dura quebrada.
    pub fn de_json(json: &str) -> Resultado<Self> {
        if json.len() > LIMITE_DE_BYTES {
            return Err(invalido("passa de 64 KiB"));
        }
        let m: Self = serde_json::from_str(json).map_err(|e| {
            // O serde cita o campo e a posição, nunca o valor de um campo de texto.
            invalido(format!("não é um JSON válido para o roteiro escrito ({e})"))
        })?;
        m.validar()?;
        Ok(m)
    }

    /// As regras duras.
    ///
    /// # Errors
    /// A primeira regra quebrada.
    pub fn validar(&self) -> Resultado<()> {
        validar_nome_kebab(&self.nome)?;
        if self.descricao.chars().count() > 500 || tem_caractere_perigoso(&self.descricao, false) {
            return Err(invalido(
                "a descrição passa de 500 caracteres ou tem caractere de controle ou de direção",
            ));
        }
        if tem_caractere_perigoso(&self.url_inicial, false) {
            return Err(invalido(
                "`url_inicial` tem caractere de controle ou de direção",
            ));
        }
        if self.hosts.is_empty() {
            return Err(invalido("`hosts` não pode ser vazio"));
        }
        if self.hosts.len() > 20 {
            return Err(invalido("`hosts` passa de 20 entradas"));
        }
        for h in &self.hosts {
            validar_host(h)?;
        }
        let host_inicial = host_da_url_https(&self.url_inicial)?;
        if !self.hosts.contains(&host_inicial) {
            return Err(invalido("o host de `url_inicial` não está em `hosts`"));
        }
        for (campo, rotulo) in [
            ("rotulo_senha", &self.rotulo_senha),
            ("rotulo_cpf", &self.rotulo_cpf),
        ] {
            if let Some(r) = rotulo
                && (r.trim().is_empty()
                    || r.chars().count() > 80
                    || tem_caractere_perigoso(r, false))
            {
                return Err(invalido(format!(
                    "`{campo}` tem de ter de 1 a 80 caracteres, sem controle nem direção"
                )));
            }
        }
        if let Mfa::Email { remetente, formato } = &self.mfa {
            if self.movimenta_dinheiro {
                return Err(invalido(
                    "MFA por e-mail só para roteiro que não movimenta dinheiro (`movimenta_dinheiro` tem de ser false)",
                ));
            }
            validar_remetente(remetente)?;
            validar_formato(formato)?;
        }
        if let Some(h) = self.agenda_horas
            && h < 1
        {
            return Err(invalido("`agenda_horas` tem de ser pelo menos 1"));
        }
        Ok(())
    }

    /// O JSON canônico (a ordem dos campos é a do struct): o que o banco guarda e o que o
    /// checksum cobre, para que espaços e ordem do arquivo não criem versão nova.
    ///
    /// # Errors
    /// Só se o serde falhar (não deveria).
    pub fn json_canonico(&self) -> Resultado<String> {
        serde_json::to_string(self).map_err(|e| invalido(format!("não serializa ({e})")))
    }

    /// O SHA-256 (hexadecimal) do par manifesto canônico + fonte. O tamanho do manifesto entra
    /// antes dos bytes, para que dois pares diferentes nunca somem os mesmos bytes.
    ///
    /// # Errors
    /// Só se o serde falhar.
    pub fn checksum_com(&self, fonte: &str) -> Resultado<String> {
        let json = self.json_canonico()?;
        let mut h = Sha256::new();
        h.update((json.len() as u64).to_be_bytes());
        h.update(json.as_bytes());
        h.update(fonte.as_bytes());
        Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// kebab-case: minúsculas e dígitos separados por hífen simples, de 1 a 60 caracteres.
fn validar_nome_kebab(nome: &str) -> Resultado<()> {
    let ok = !nome.is_empty()
        && nome.len() <= 60
        && nome.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
    if ok {
        Ok(())
    } else {
        Err(invalido(
            "`nome` tem de ser kebab-case (minúsculas, dígitos e hífen, até 60 caracteres), por exemplo `contracheque-exemplo`",
        ))
    }
}

/// Os finais de nome que só existem numa rede interna: não são sites.
const FINAIS_INTERNOS: &[&str] = &[
    "local",
    "localhost",
    "internal",
    "lan",
    "corp",
    "home",
    "localdomain",
    "intranet",
];

/// Só hostname **público**: rótulos de letras minúsculas, dígitos e hífen, separados por ponto,
/// com pelo menos um ponto. Sem esquema, caminho, porta, usuário nem curinga; sem IP literal
/// (v4 ou v6), `localhost`, nome sem ponto nem final interno (`.local`, `.internal`, `.lan`,
/// `.corp`, `.home`, `.localdomain`): um roteiro não aponta o navegador (nem a senha) para a
/// rede da casa.
fn validar_host(h: &str) -> Resultado<()> {
    let forma_ok = !h.is_empty()
        && h.len() <= 253
        && h.split('.').all(|r| {
            !r.is_empty()
                && r.len() <= 63
                && !r.starts_with('-')
                && !r.ends_with('-')
                && r.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    if !forma_ok {
        return Err(invalido(
            "cada item de `hosts` tem de ser só o nome do servidor em minúsculas (sem https://, caminho, porta nem curinga)",
        ));
    }
    let final_do_nome = h.rsplit('.').next().unwrap_or("");
    let e_ip =
        h.parse::<std::net::IpAddr>().is_ok() || final_do_nome.bytes().all(|b| b.is_ascii_digit());
    if e_ip || !h.contains('.') || h == "localhost" || FINAIS_INTERNOS.contains(&final_do_nome) {
        return Err(invalido(
            "cada item de `hosts` tem de ser o nome público de um site (com ponto): sem endereço IP, `localhost`, nome sem ponto nem final interno como `.local`, `.internal` ou `.lan`",
        ));
    }
    Ok(())
}

/// O host de uma URL `https://`; recusa outro esquema, usuário e senha na URL.
fn host_da_url_https(url: &str) -> Resultado<String> {
    let Some(resto) = url.strip_prefix("https://") else {
        return Err(invalido("`url_inicial` tem de começar com https://"));
    };
    let autoridade = resto.split(['/', '?', '#']).next().unwrap_or("");
    if autoridade.contains('@') {
        return Err(invalido("`url_inicial` não pode ter usuário nem senha"));
    }
    let host = match autoridade.rsplit_once(':') {
        Some((h, "443")) => h,
        Some((_, porta)) if !porta.is_empty() && porta.bytes().all(|b| b.is_ascii_digit()) => {
            return Err(invalido(
                "`url_inicial` só pode usar a porta 443 (a padrão do https), ou nenhuma",
            ));
        }
        Some(_) => return Err(invalido("`url_inicial` tem uma porta inválida")),
        None => autoridade,
    };
    let host = host.to_ascii_lowercase();
    validar_host(&host).map_err(|_| invalido("o host de `url_inicial` é inválido"))?;
    Ok(host)
}

/// Um e-mail simples e fixo: `parte@dominio.tld`, sem espaço, curinga nem lista.
fn validar_remetente(r: &str) -> Resultado<()> {
    let ok = r.len() <= 254
        && r.split_once('@').is_some_and(|(local, dominio)| {
            !local.is_empty()
                && local
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+'))
                && dominio.contains('.')
                && validar_host(&dominio.to_ascii_lowercase()).is_ok()
        });
    if ok {
        Ok(())
    } else {
        Err(invalido(
            "`mfa.remetente` tem de ser um endereço de e-mail único e fixo (sem espaço, lista nem curinga)",
        ))
    }
}

/// O formato do código: só `\d`, `\w`, letras, dígitos, classes `[A-Z0-9]` e quantificador
/// **exato** `{n}` com 4 <= n <= 12 (`\d{6}`). Nada de `.`, `*`, `+`, `{1,99}`, `\s`, negação: o
/// código tem tamanho fixo e a busca no Gmail não pode virar "qualquer texto".
fn validar_formato(f: &str) -> Resultado<()> {
    let erro = || {
        invalido(
            "`mfa.formato` tem de casar só o código (dígitos e letras, tamanho fixo, como \\d{6}): sem `.`, `*`, `+` nem `\\s`",
        )
    };
    if f.is_empty() || f.len() > 64 {
        return Err(erro());
    }
    let cs: Vec<char> = f.chars().collect();
    let mut i = 0;
    let mut em_classe = false;
    let mut tem_quantificador_numerico = false;
    while i < cs.len() {
        let c = cs[i];
        match c {
            '\\' => {
                i += 1;
                if !matches!(cs.get(i), Some('d' | 'w' | 'b')) {
                    return Err(erro());
                }
                if em_classe && cs[i] == 'b' {
                    return Err(erro());
                }
            }
            '[' if !em_classe => {
                em_classe = true;
                if cs.get(i + 1) == Some(&'^') {
                    return Err(erro());
                }
            }
            ']' if em_classe => em_classe = false,
            c if c.is_ascii_alphanumeric() || c == '-' => {}
            '{' if !em_classe => {
                let fim = cs[i..].iter().position(|&x| x == '}').ok_or_else(erro)?;
                let dentro: String = cs[i + 1..i + fim].iter().collect();
                let exato = !dentro.is_empty()
                    && dentro.len() <= 2
                    && dentro.bytes().all(|b| b.is_ascii_digit())
                    && dentro.parse::<u32>().is_ok_and(|n| (4..=12).contains(&n));
                if !exato {
                    return Err(erro());
                }
                tem_quantificador_numerico = true;
                i += fim;
            }
            '(' | ')' | '^' | '$' if !em_classe => {}
            _ => return Err(erro()),
        }
        i += 1;
    }
    if em_classe {
        return Err(erro());
    }
    let re =
        Regex::new(f).map_err(|_| invalido("`mfa.formato` não é uma expressão regular válida"))?;
    // Um código tem tamanho fixo e conhecido: tem de haver quantificador exato, e a expressão
    // não pode casar o texto vazio.
    if re.is_match("") || !tem_quantificador_numerico {
        return Err(erro());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    fn base() -> Manifesto {
        Manifesto {
            nome: "contracheque-exemplo".into(),
            descricao: "Baixa o contracheque do mês".into(),
            url_inicial: "https://portal.exemplo.test/entrar".into(),
            hosts: vec!["portal.exemplo.test".into()],
            rotulo_senha: Some("senha:portal-exemplo".into()),
            rotulo_cpf: None,
            mfa: Mfa::Nenhum,
            movimenta_dinheiro: false,
            classe: Classe::Consulta,
            agenda_horas: Some(168),
        }
    }

    fn email(formato: &str) -> Manifesto {
        Manifesto {
            mfa: Mfa::Email {
                remetente: "nao-responda@portal.exemplo.test".into(),
                formato: formato.into(),
            },
            ..base()
        }
    }

    #[test]
    fn o_manifesto_do_contrato_le_e_valida() {
        let json = r#"{
          "nome": "contracheque-exemplo",
          "descricao": "texto",
          "url_inicial": "https://portal.exemplo.test/entrar",
          "hosts": ["portal.exemplo.test"],
          "rotulo_senha": "senha:portal-exemplo",
          "rotulo_cpf": null,
          "mfa": { "tipo": "email", "remetente": "nao-responda@portal.exemplo.test", "formato": "\\d{6}" },
          "movimenta_dinheiro": false,
          "classe": "consulta",
          "agenda_horas": 168
        }"#;
        let m = Manifesto::de_json(json).unwrap();
        assert_eq!(m.agenda_horas, Some(168));
        assert!(matches!(m.mfa, Mfa::Email { .. }));
        assert_eq!(m.classe, Classe::Consulta);
    }

    #[test]
    fn mfa_nenhum_e_manual_e_classe_padrao() {
        let json = r#"{"nome":"a","url_inicial":"https://x.exemplo.test/","hosts":["x.exemplo.test"],
          "mfa":{"tipo":"manual"},"movimenta_dinheiro":true}"#;
        let m = Manifesto::de_json(json).unwrap();
        assert_eq!(m.mfa, Mfa::Manual);
        assert_eq!(m.classe, Classe::Consulta);
        assert_eq!(m.agenda_horas, None);
        assert_eq!(m.rotulo_senha, None);
    }

    #[test]
    fn classe_atualizar_conexao_vale_e_outra_nao() {
        let ok = r#"{"nome":"a","url_inicial":"https://x.exemplo.test/","hosts":["x.exemplo.test"],
          "mfa":{"tipo":"nenhum"},"movimenta_dinheiro":false,"classe":"atualizar_conexao"}"#;
        assert_eq!(
            Manifesto::de_json(ok).unwrap().classe,
            Classe::AtualizarConexao
        );
        let ruim = ok.replace("atualizar_conexao", "pagar");
        assert!(Manifesto::de_json(&ruim).is_err());
    }

    #[test]
    fn movimenta_dinheiro_e_obrigatorio_e_campo_desconhecido_e_recusado() {
        let sem = r#"{"nome":"a","url_inicial":"https://x.exemplo.test/","hosts":["x.exemplo.test"],
          "mfa":{"tipo":"email","remetente":"a@x.exemplo.test","formato":"\\d{6}"}}"#;
        assert!(Manifesto::de_json(sem).is_err());
        let extra = r#"{"nome":"a","url_inicial":"https://x.exemplo.test/","hosts":["x.exemplo.test"],
          "mfa":{"tipo":"nenhum"},"movimenta_dinheiro":false,"senha":"nao"}"#;
        assert!(Manifesto::de_json(extra).is_err());
    }

    #[test]
    fn nome_so_em_kebab_case() {
        for ok in ["a", "contracheque-exemplo", "meu-pluggy-2", "2024-fgts"] {
            let m = Manifesto {
                nome: ok.into(),
                ..base()
            };
            assert!(m.validar().is_ok(), "{ok}");
        }
        for ruim in [
            "",
            "Contracheque",
            "contra_cheque",
            "contra cheque",
            "-a",
            "a-",
            "a--b",
            "a/b",
            "ação",
            &"a".repeat(61),
        ] {
            let m = Manifesto {
                nome: ruim.into(),
                ..base()
            };
            assert!(m.validar().is_err(), "{ruim}");
        }
    }

    #[test]
    fn hosts_nao_vazio_e_so_hostname() {
        let vazio = Manifesto {
            hosts: vec![],
            ..base()
        };
        assert!(vazio.validar().is_err());
        for ruim in [
            "https://portal.exemplo.test",
            "portal.exemplo.test/entrar",
            "portal.exemplo.test:443",
            "*.exemplo.test",
            "Portal.Exemplo.test",
            "portal..exemplo.test",
            "-portal.exemplo.test",
            "",
            "user@portal.exemplo.test",
            // IP literal, localhost, sem ponto e finais de rede interna
            "127.0.0.1",
            "10.0.0.5",
            "192.168.0.1",
            "1.2.3.4",
            "[::1]",
            "::1",
            "localhost",
            "banco",
            "roteador",
            "nas.local",
            "painel.internal",
            "casa.lan",
            "ad.corp",
            "tv.home",
            "host.localdomain",
            "api.localhost",
        ] {
            let m = Manifesto {
                hosts: vec![ruim.into()],
                ..base()
            };
            assert!(m.validar().is_err(), "{ruim}");
        }
        let dois = Manifesto {
            hosts: vec!["portal.exemplo.test".into(), "auth.exemplo.test".into()],
            ..base()
        };
        assert!(dois.validar().is_ok());
    }

    #[test]
    fn url_inicial_https_e_dentro_dos_hosts() {
        for ruim in [
            "http://portal.exemplo.test/entrar",
            "ftp://portal.exemplo.test/",
            "portal.exemplo.test/entrar",
            "https://outro.exemplo.test/entrar",
            "https://portal.exemplo.test.mal.test/",
            "https://usuario:x@portal.exemplo.test/",
            "https://portal.exemplo.test@mal.test/",
            "https://portal.exemplo.test:abc/",
            // só a porta 443 (ou nenhuma)
            "https://portal.exemplo.test:8443/x",
            "https://portal.exemplo.test:80/",
            "https://portal.exemplo.test:4443/",
            // controle e direção na URL
            "https://portal.exemplo.test/\u{1b}[3A",
            "https://portal.exemplo.test/a\rb",
            "https://portal.exemplo.test/\u{202e}fdp",
            "",
        ] {
            let m = Manifesto {
                url_inicial: ruim.into(),
                ..base()
            };
            assert!(m.validar().is_err(), "{ruim}");
        }
        for ok in [
            "https://portal.exemplo.test",
            "https://portal.exemplo.test/entrar?a=1#x",
            "https://portal.exemplo.test:443/x",
            "https://PORTAL.exemplo.test/x",
        ] {
            let m = Manifesto {
                url_inicial: ok.into(),
                ..base()
            };
            assert!(m.validar().is_ok(), "{ok}");
        }
    }

    #[test]
    fn mfa_email_exige_nao_movimentar_dinheiro() {
        let mut m = email(r"\d{6}");
        assert!(m.validar().is_ok());
        m.movimenta_dinheiro = true;
        let e = m.validar().unwrap_err().to_string();
        assert!(e.contains("não movimenta dinheiro"), "{e}");
        // manual e nenhum não têm essa restrição
        let manual = Manifesto {
            mfa: Mfa::Manual,
            movimenta_dinheiro: true,
            ..base()
        };
        assert!(manual.validar().is_ok());
    }

    #[test]
    fn mfa_email_exige_remetente_valido() {
        for ruim in [
            "",
            "sem-arroba",
            "@portal.exemplo.test",
            "a@semponto",
            "a b@portal.exemplo.test",
            "*@portal.exemplo.test",
            "a@b.test, c@d.test",
            "a@*.exemplo.test",
        ] {
            let m = Manifesto {
                mfa: Mfa::Email {
                    remetente: ruim.into(),
                    formato: r"\d{6}".into(),
                },
                ..base()
            };
            assert!(m.validar().is_err(), "{ruim}");
        }
    }

    #[test]
    fn mfa_email_formato_so_casa_o_codigo() {
        for ok in [
            r"\d{6}",
            r"\d{4}",
            r"\d{12}",
            r"[A-Z0-9]{6}",
            r"\w{8}",
            r"G-\d{6}",
            "[0-9]{6}",
        ] {
            assert!(email(ok).validar().is_ok(), "{ok}");
        }
        for ruim in [
            "",
            ".*",
            r".{6}",
            r"\d+",
            r"\d*",
            r"\d{6}.*",
            r"\s\d{6}",
            r"\D{6}",
            r"\S{6}",
            r"[^a]{6}",
            r"\d{0,6}",
            r"(",
            r"\d{6",
            r"\d{a}",
            r"\d{}",
            r"[a-z",
            r"\d{6}|",
            r"\d{6}\.",
            // só quantificador exato {n}, 4 <= n <= 12
            r"\d{1,99}",
            r"\d{4,8}",
            r"\d{3}",
            r"\d{13}",
            r"\d{99}",
            r"\d+",
            r"\d\d\d\d\d\d",
            "abcdef",
        ] {
            assert!(email(ruim).validar().is_err(), "{ruim}");
        }
        assert!(email(&"a".repeat(65)).validar().is_err());
    }

    #[test]
    fn controle_e_direcao_sao_recusados_nos_campos_de_texto() {
        for ruim in [
            "a\u{1b}[3Ab",
            "a\rb",
            "a\nb",
            "a\u{202e}b",
            "a\u{2066}b",
            "a\u{200f}b",
            "a\u{85}b",
            "a\0b",
        ] {
            let m = Manifesto {
                descricao: ruim.into(),
                ..base()
            };
            assert!(m.validar().is_err(), "descricao {ruim:?}");
            let m = Manifesto {
                rotulo_senha: Some(ruim.into()),
                ..base()
            };
            assert!(m.validar().is_err(), "rotulo {ruim:?}");
            let m = Manifesto {
                url_inicial: format!("https://portal.exemplo.test/{ruim}"),
                ..base()
            };
            assert!(m.validar().is_err(), "url {ruim:?}");
            let m = Manifesto {
                nome: ruim.into(),
                ..base()
            };
            assert!(m.validar().is_err(), "nome {ruim:?}");
            let m = Manifesto {
                hosts: vec![format!("portal.exemplo.test{ruim}")],
                ..base()
            };
            assert!(m.validar().is_err(), "host {ruim:?}");
            let m = Manifesto {
                mfa: Mfa::Email {
                    remetente: format!("a{ruim}@portal.exemplo.test"),
                    formato: r"\d{6}".into(),
                },
                ..base()
            };
            assert!(m.validar().is_err(), "remetente {ruim:?}");
            let m = Manifesto {
                mfa: Mfa::Email {
                    remetente: "a@portal.exemplo.test".into(),
                    formato: format!(r"\d{{6}}{ruim}"),
                },
                ..base()
            };
            assert!(m.validar().is_err(), "formato {ruim:?}");
        }
    }

    #[test]
    fn o_escapador_troca_controle_e_direcao_por_texto_visivel() {
        let sujo = "ok\u{1b}[3A\rfim\u{202e}fdp\n\tx\0";
        let e = escapar_para_terminal(sujo);
        assert_eq!(e, "ok\\x1b[3A\\x0dfim\\u{202e}fdp\\x0a\\x09x\\x00");
        assert!(!e.chars().any(|c| c.is_control() || e_bidi(c)));
        // o fonte mantém quebra de linha e tabulação, escapa o resto
        let f = escapar_fonte_para_terminal(sujo);
        assert_eq!(f, "ok\\x1b[3A\\x0dfim\\u{202e}fdp\n\tx\\x00");
        assert_eq!(escapar_para_terminal("açaí 日本"), "açaí 日本");
        assert!(tem_caractere_perigoso("a\rb", true));
        assert!(!tem_caractere_perigoso("a\n\tb", true));
        assert!(tem_caractere_perigoso("a\nb", false));
    }

    #[test]
    fn agenda_horas_opcional_e_pelo_menos_um() {
        for ok in [None, Some(1), Some(168)] {
            let m = Manifesto {
                agenda_horas: ok,
                ..base()
            };
            assert!(m.validar().is_ok(), "{ok:?}");
        }
        let m = Manifesto {
            agenda_horas: Some(0),
            ..base()
        };
        assert!(m.validar().is_err());
    }

    #[test]
    fn rotulos_vazios_ou_com_controle_sao_recusados_sem_eco() {
        let m = Manifesto {
            rotulo_senha: Some("  ".into()),
            ..base()
        };
        assert!(m.validar().is_err());
        let m = Manifesto {
            rotulo_cpf: Some("a\nb".into()),
            ..base()
        };
        let e = m.validar().unwrap_err().to_string();
        assert!(!e.contains("a\nb"), "{e}");
    }

    #[test]
    fn a_mensagem_de_erro_nao_ecoa_o_rotulo_da_senha() {
        let m = Manifesto {
            rotulo_senha: Some("senha:segredo-do-portal".into()),
            hosts: vec![],
            ..base()
        };
        let e = m.validar().unwrap_err().to_string();
        assert!(!e.contains("segredo-do-portal"), "{e}");
    }

    #[test]
    fn checksum_ignora_formatacao_do_arquivo_e_muda_com_o_conteudo() {
        let a = base();
        let c1 = a.checksum_com("print(1)").unwrap();
        assert_eq!(c1.len(), 64);
        // ler o mesmo manifesto de um JSON com outra ordem e espaços dá o mesmo checksum
        let json = serde_json::to_string_pretty(&a).unwrap();
        let b = Manifesto::de_json(&json).unwrap();
        assert_eq!(b.checksum_com("print(1)").unwrap(), c1);
        assert_ne!(a.checksum_com("print(2)").unwrap(), c1);
        let outro = Manifesto {
            descricao: "outra".into(),
            ..base()
        };
        assert_ne!(outro.checksum_com("print(1)").unwrap(), c1);
    }

    #[test]
    fn manifesto_grande_demais_e_recusado() {
        let json = format!(
            r#"{{"nome":"a","descricao":"{}"}}"#,
            "x".repeat(LIMITE_DE_BYTES)
        );
        assert!(Manifesto::de_json(&json).is_err());
    }
}
