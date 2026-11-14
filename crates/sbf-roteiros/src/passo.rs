//! O roteiro: uma lista versionada de passos em JSON (spec 10, "Gravação").
//!
//! Um passo guarda **só a referência** ao chaveiro (o rótulo), nunca a senha nem o CPF: o
//! roteiro gravado é texto comum no `roteiros.sqlite` e tem de poder ser lido sem susto.

use serde::{Deserialize, Serialize};

/// A versão do formato do JSON do roteiro.
pub const FORMATO: u32 = 1;

fn formato_padrao() -> u32 {
    FORMATO
}

/// Um passo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Passo {
    /// Abre a URL (conferida na lista de bloqueio).
    Navegar { url: String },
    /// Espera o seletor aparecer, visível.
    EsperarSeletor {
        seletor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segundos: Option<u64>,
    },
    /// Clica no elemento (conferido na lista de bloqueio, no executor e no navegador).
    Clicar { seletor: String },
    /// Preenche um campo com texto fixo (nunca um campo de senha).
    Preencher { seletor: String, texto: String },
    /// Preenche com a senha do chaveiro de rótulo `rotulo` (senhas de sites). `host` é o site
    /// em que o usuário aprovou usar a senha: o executor só digita se a página atual é desse
    /// host (a senha não vai para um endereço que o usuário não viu).
    PreencherSenha {
        seletor: String,
        rotulo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    /// Preenche com o CPF do chaveiro (mesma regra do `host`).
    PreencherCpf {
        seletor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    /// Clica no elemento que dispara um download e espera o arquivo.
    Baixar {
        seletor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nome: Option<String>,
    },
    /// Lê uma tabela da página e a grava como CSV (também entra no inventário).
    LerTabela { seletor: String, nome: String },
    /// Para e passa a vez ao usuário (MFA, CAPTCHA, confirmação).
    PausaParaUsuario { motivo: String },
}

impl Passo {
    /// Uma frase curta para a janela e o histórico. Sem valores.
    #[must_use]
    pub fn descricao(&self) -> String {
        match self {
            Self::Navegar { url } => format!("Abrir {}", sem_consulta(url)),
            Self::EsperarSeletor { seletor, .. } => format!("Esperar {seletor}"),
            Self::Clicar { seletor } => format!("Clicar em {seletor}"),
            Self::Preencher { seletor, .. } => format!("Preencher {seletor}"),
            Self::PreencherSenha {
                seletor,
                rotulo,
                host,
            } => format!(
                "Preencher {seletor} com a senha \"{rotulo}\" do chaveiro{}",
                so_no_host(host.as_deref())
            ),
            Self::PreencherCpf { seletor, host } => format!(
                "Preencher {seletor} com o CPF do chaveiro{}",
                so_no_host(host.as_deref())
            ),
            Self::Baixar { seletor, .. } => format!("Baixar por {seletor}"),
            Self::LerTabela { nome, .. } => format!("Ler a tabela \"{nome}\""),
            Self::PausaParaUsuario { motivo } => format!("Sua vez: {motivo}"),
        }
    }
}

fn so_no_host(host: Option<&str>) -> String {
    host.map(|h| format!(" (só em {h})")).unwrap_or_default()
}

/// O host de uma URL `http(s)`, em minúsculas, sem porta nem credenciais. `None` se não é uma
/// URL de rede (`about:blank`, `chrome-error:`…).
#[must_use]
pub fn host_de(url: &str) -> Option<String> {
    let resto = url.split_once("://")?.1;
    let autoridade = resto.split(['/', '?', '#']).next()?;
    let sem_credenciais = autoridade.rsplit('@').next()?;
    let host = if let Some(v6) = sem_credenciais.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        sem_credenciais.split(':').next()?
    };
    let host = host.trim().to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Segredo (senha, CPF) só vai a uma página `https`; `http` só no próprio computador
/// (`127.0.0.1`, `localhost`, `::1`: o site falso dos testes e da demonstração).
#[must_use]
pub fn esquema_aceito_para_segredo(url: &str) -> bool {
    let u = url.trim_start().to_ascii_lowercase();
    if u.starts_with("https://") {
        return host_de(&u).is_some();
    }
    u.starts_with("http://")
        && matches!(
            host_de(&u).as_deref(),
            Some("127.0.0.1" | "localhost" | "::1")
        )
}

fn sem_consulta(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

/// Um roteiro: o que se grava na versão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roteiro {
    #[serde(default = "formato_padrao")]
    pub formato: u32,
    pub nome: String,
    #[serde(default)]
    pub descricao: String,
    pub passos: Vec<Passo>,
}

impl Roteiro {
    /// Valida o que o JSON não garante: nome, formato conhecido, passos sem segredo literal.
    ///
    /// # Errors
    /// A primeira regra violada.
    pub fn validar(&self) -> Result<(), String> {
        if self.formato > FORMATO {
            return Err(format!(
                "formato {} é mais novo que o conhecido",
                self.formato
            ));
        }
        validar_nome(&self.nome)?;
        for (i, p) in self.passos.iter().enumerate() {
            let vazio = match p {
                Passo::Navegar { url } => url.trim().is_empty(),
                Passo::EsperarSeletor { seletor, .. }
                | Passo::Clicar { seletor }
                | Passo::PreencherCpf { seletor, .. }
                | Passo::Baixar { seletor, .. }
                | Passo::LerTabela { seletor, .. }
                | Passo::Preencher { seletor, .. } => seletor.trim().is_empty(),
                Passo::PreencherSenha {
                    seletor, rotulo, ..
                } => seletor.trim().is_empty() || rotulo.trim().is_empty(),
                Passo::PausaParaUsuario { motivo } => motivo.trim().is_empty(),
            };
            if vazio {
                return Err(format!("o passo {} está incompleto", i + 1));
            }
            if let Passo::PreencherSenha { host, .. } | Passo::PreencherCpf { host, .. } = p
                && host.as_deref().is_none_or(|h| h.trim().is_empty())
            {
                return Err(format!(
                    "o passo {} usa o chaveiro mas não diz em que site (host) a senha vale",
                    i + 1
                ));
            }
        }
        Ok(())
    }
}

/// O nome de um roteiro: de 1 a 60 caracteres, sem `/`, sem controle (entra na origem
/// `roteiro:<nome>/<execução>/<arquivo>`).
///
/// # Errors
/// Se o nome não serve.
pub fn validar_nome(nome: &str) -> Result<(), String> {
    if nome.trim().is_empty() || nome.chars().count() > 60 {
        return Err("o nome do roteiro tem de ter de 1 a 60 caracteres".into());
    }
    if nome.contains('/') || nome.chars().any(char::is_control) {
        return Err("o nome do roteiro não pode ter barra nem caractere de controle".into());
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_json_do_passo_de_senha_guarda_so_o_rotulo() {
        let p = Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("banco.exemplo".into()),
        };
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(
            j,
            r##"{"tipo":"preencher_senha","seletor":"#senha","rotulo":"banco-ficticio","host":"banco.exemplo"}"##
        );
        assert_eq!(serde_json::from_str::<Passo>(&j).unwrap(), p);
    }

    #[test]
    fn roteiro_vai_e_volta_e_valida() {
        let r = Roteiro {
            formato: FORMATO,
            nome: "extratos".into(),
            descricao: String::new(),
            passos: vec![
                Passo::Navegar {
                    url: "http://127.0.0.1:1/".into(),
                },
                Passo::Baixar {
                    seletor: "#baixar".into(),
                    nome: None,
                },
            ],
        };
        let j = serde_json::to_string(&r).unwrap();
        let volta: Roteiro = serde_json::from_str(&j).unwrap();
        assert_eq!(volta, r);
        volta.validar().unwrap();
    }

    #[test]
    fn nome_com_barra_ou_passo_vazio_nao_valida() {
        assert!(validar_nome("a/b").is_err());
        assert!(validar_nome("").is_err());
        let r = Roteiro {
            formato: FORMATO,
            nome: "x".into(),
            descricao: String::new(),
            passos: vec![Passo::Clicar {
                seletor: " ".into(),
            }],
        };
        assert!(r.validar().is_err());
    }

    #[test]
    fn host_de_url() {
        assert_eq!(
            host_de("https://Banco.Exemplo.com.br:8443/a?b#c").as_deref(),
            Some("banco.exemplo.com.br")
        );
        assert_eq!(
            host_de("http://u:p@127.0.0.1:80/x").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(host_de("http://[::1]:9/x").as_deref(), Some("::1"));
        assert_eq!(host_de("about:blank"), None);
        assert_eq!(
            host_de("chrome-error://chromewebdata/"),
            Some("chromewebdata".into())
        );
        assert_eq!(
            host_de("https://evil.example.com@good.example/").as_deref(),
            Some("good.example")
        );
    }

    #[test]
    fn segredo_so_em_https_ou_no_proprio_computador() {
        assert!(esquema_aceito_para_segredo("https://banco.example/login"));
        assert!(esquema_aceito_para_segredo("http://127.0.0.1:8080/login"));
        assert!(esquema_aceito_para_segredo("http://localhost:3000/"));
        assert!(!esquema_aceito_para_segredo("http://banco.example/login"));
        assert!(!esquema_aceito_para_segredo("ftp://banco.example/"));
        assert!(!esquema_aceito_para_segredo("about:blank"));
        assert!(!esquema_aceito_para_segredo(
            "chrome-error://chromewebdata/"
        ));
        assert!(!esquema_aceito_para_segredo(""));
    }

    #[test]
    fn passo_com_chaveiro_sem_host_nao_valida() {
        for p in [
            Passo::PreencherSenha {
                seletor: "#s".into(),
                rotulo: "r".into(),
                host: None,
            },
            Passo::PreencherCpf {
                seletor: "#c".into(),
                host: Some(" ".into()),
            },
        ] {
            let r = Roteiro {
                formato: FORMATO,
                nome: "x".into(),
                descricao: String::new(),
                passos: vec![p],
            };
            assert!(r.validar().is_err());
        }
    }

    #[test]
    fn a_descricao_da_navegacao_tira_a_consulta() {
        let d = Passo::Navegar {
            url: "http://x/y?token=segredo#a".into(),
        }
        .descricao();
        assert!(!d.contains("segredo"));
    }
}
