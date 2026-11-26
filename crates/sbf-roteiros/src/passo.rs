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

impl Passo {
    /// A descrição **completa**, para o usuário aprovar: a URL inteira (com a consulta), o
    /// texto que será digitado. Só vai à tela dele; o que vai à IA e ao histórico é
    /// [`Self::descricao`], que não leva a consulta nem o texto.
    #[must_use]
    pub fn descricao_completa(&self) -> String {
        match self {
            Self::Navegar { url } => format!("Abrir {url}"),
            Self::Preencher { seletor, texto } => {
                format!("Preencher {seletor} com o texto \"{texto}\"")
            }
            Self::Baixar { seletor, nome } => match nome {
                Some(n) => format!("Baixar por {seletor} (salvar como \"{n}\")"),
                None => format!("Baixar por {seletor}"),
            },
            outro => outro.descricao(),
        }
    }

    /// Troca o seletor do passo, se ele aparece como chave de `mapa` (o seletor mascarado que a
    /// IA viu volta ao de verdade).
    #[must_use]
    pub fn com_seletor_mapeado(mut self, mapa: &std::collections::HashMap<String, String>) -> Self {
        let (Self::EsperarSeletor { seletor, .. }
        | Self::Clicar { seletor }
        | Self::Preencher { seletor, .. }
        | Self::PreencherSenha { seletor, .. }
        | Self::PreencherCpf { seletor, .. }
        | Self::Baixar { seletor, .. }
        | Self::LerTabela { seletor, .. }) = &mut self
        else {
            return self;
        };
        if let Some(original) = mapa.get(seletor.as_str()) {
            seletor.clone_from(original);
        }
        self
    }

    /// O host que este passo visita ou em que aprova usar o chaveiro, se algum.
    fn host_do_passo(&self) -> Option<String> {
        match self {
            Self::Navegar { url } => host_de(url),
            Self::PreencherSenha { host, .. } | Self::PreencherCpf { host, .. } => host
                .as_deref()
                .map(str::to_ascii_lowercase)
                .filter(|h| !h.is_empty()),
            _ => None,
        }
    }
}

/// Uma linha do "antes e depois" de um conserto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeDiferenca {
    pub marca: Marca,
    /// A descrição completa do passo.
    pub texto: String,
    /// O host que só este passo traz (nenhum passo antigo o visita nem o aprova).
    pub host_novo: Option<String>,
}

/// O que aconteceu com o passo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marca {
    Igual,
    Removido,
    Novo,
}

/// O "antes e depois" de um conserto, alinhado pela maior sequência de passos em comum, com os
/// passos novos e os **sites novos** marcados: o usuário vê o que a IA acrescentou antes de
/// aprovar.
#[must_use]
pub fn diferenca(antes: &[Passo], depois: &[Passo]) -> Vec<LinhaDeDiferenca> {
    let (n, m) = (antes.len(), depois.len());
    // maior subsequência comum
    let mut lcs = vec![vec![0_usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if antes[i] == depois[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let hosts_antigos: Vec<String> = antes.iter().filter_map(Passo::host_do_passo).collect();
    let (mut i, mut j) = (0, 0);
    let mut saida = Vec::new();
    while i < n || j < m {
        if i < n && j < m && antes[i] == depois[j] {
            saida.push(LinhaDeDiferenca {
                marca: Marca::Igual,
                texto: antes[i].descricao_completa(),
                host_novo: None,
            });
            i += 1;
            j += 1;
        } else if i < n && (j >= m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            saida.push(LinhaDeDiferenca {
                marca: Marca::Removido,
                texto: antes[i].descricao_completa(),
                host_novo: None,
            });
            i += 1;
        } else {
            let host_novo = depois[j]
                .host_do_passo()
                .filter(|h| !hosts_antigos.contains(h));
            saida.push(LinhaDeDiferenca {
                marca: Marca::Novo,
                texto: depois[j].descricao_completa(),
                host_novo,
            });
            j += 1;
        }
    }
    saida
}

fn so_no_host(host: Option<&str>) -> String {
    host.map(|h| format!(" (só em {h})")).unwrap_or_default()
}

pub use bancada_navegador::url::{esquema_aceito_para_segredo, host_de, url_navegavel};

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
            if let Passo::Navegar { url } = p
                && !url.trim().is_empty()
                && !url_navegavel(url)
            {
                return Err(format!(
                    "o passo {} abre um endereço que não é http nem https",
                    i + 1
                ));
            }
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
    fn a_aprovacao_mostra_a_url_inteira_e_o_texto_digitado() {
        let n = Passo::Navegar {
            url: "https://b.example/extratos?mes=10&conta=1".into(),
        };
        assert!(n.descricao_completa().contains("mes=10&conta=1"));
        assert!(!n.descricao().contains("mes=10"));
        let p = Passo::Preencher {
            seletor: "#mes".into(),
            texto: "outubro".into(),
        };
        assert!(p.descricao_completa().contains("outubro"));
        assert!(!p.descricao().contains("outubro"));
    }

    #[test]
    fn so_http_e_https_sao_navegaveis() {
        assert!(url_navegavel("https://a.example/x"));
        assert!(url_navegavel("HTTP://127.0.0.1:1/"));
        for u in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "chrome://settings",
            "about:blank",
            "https://",
            "",
        ] {
            assert!(!url_navegavel(u), "{u}");
        }
        let r = Roteiro {
            formato: FORMATO,
            nome: "x".into(),
            descricao: String::new(),
            passos: vec![Passo::Navegar {
                url: "file:///etc/passwd".into(),
            }],
        };
        assert!(r.validar().is_err());
    }

    #[test]
    fn a_diferenca_destaca_o_passo_novo_e_o_host_novo() {
        let nav = |u: &str| Passo::Navegar { url: u.into() };
        let clicar = |s: &str| Passo::Clicar { seletor: s.into() };
        let antes = vec![nav("https://banco.example/login"), clicar("#entrar")];
        let depois = vec![
            nav("https://banco.example/login"),
            clicar("#entrar"),
            nav("https://outro.example/x?a=1"),
            clicar("#baixar"),
        ];
        let d = diferenca(&antes, &depois);
        assert_eq!(d[0].marca, Marca::Igual);
        assert_eq!(d[1].marca, Marca::Igual);
        let novo_host = d.iter().find(|l| l.host_novo.is_some()).unwrap();
        assert_eq!(novo_host.host_novo.as_deref(), Some("outro.example"));
        assert_eq!(novo_host.marca, Marca::Novo);
        assert!(novo_host.texto.contains("a=1"), "a URL vai inteira");
        assert_eq!(d.iter().filter(|l| l.marca == Marca::Novo).count(), 2);
        // o que sai
        let d = diferenca(&depois, &antes);
        assert_eq!(d.iter().filter(|l| l.marca == Marca::Removido).count(), 2);
        assert!(d.iter().all(|l| l.host_novo.is_none()));
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
