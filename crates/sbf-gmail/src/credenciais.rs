//! As credenciais OAuth2 de uma conta de e-mail: `client_id`, `client_secret` e
//! `refresh_token`.
//!
//! **Onde ficam (chaveiro):** por conta, `email/<id>/client_id|client_secret|refresh_token`; e
//! as do **aplicativo** (o `client_id`/`client_secret` registrados no Google, que várias contas
//! compartilham) por tipo, `email/<tipo>/app/client_id|client_secret`.
//!
//! **Ordem de busca, campo a campo:** chaveiro da conta, chaveiro do aplicativo do tipo,
//! ambiente do processo, `.env` do diretório atual e dos de cima. O ambiente e o `.env`
//! (`GOOGLE_MAIL_CLIENT_ID`, `GOOGLE_MAIL_CLIENT_SECRET`, `GMAIL_REFRESH_TOKEN`) só valem para o
//! Gmail, e o `GMAIL_REFRESH_TOKEN` só para a conta legada (id `gmail`).
//!
//! O chaveiro vem **primeiro** de propósito: ele só é escrito por ação explícita da pessoa
//! (entrar, importar, editar), então é sempre o mais novo. Se o ambiente ou o `.env` ganhassem,
//! o token vencido de lá esconderia o do login novo. Efeito colateral: depois de um login,
//! mudar o token no `.env` não tem efeito até `sbf email sair gmail` (ou `sbf email importar`,
//! que copia do ambiente/.env para o chaveiro).
//!
//! Nada daqui vai para log, `Debug` ou mensagem de erro; o erro de falta leva só os **nomes**.

use std::path::{Path, PathBuf};

use sbf_chaveiro::{Chaveiro, Segredo};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::config::TipoDeEmail;
use crate::erro::{Erro, Resultado};
use crate::gravador::ID_LEGADO;

/// As variáveis de ambiente (as mesmas do coletor da VPS).
pub const VARIAVEIS: [&str; 3] = [
    "GOOGLE_MAIL_CLIENT_ID",
    "GOOGLE_MAIL_CLIENT_SECRET",
    "GMAIL_REFRESH_TOKEN",
];

/// Os campos guardados.
pub const CAMPO_DO_ID: &str = "client_id";
/// O client secret.
pub const CAMPO_DO_SEGREDO: &str = "client_secret";
/// O refresh token.
pub const CAMPO_DO_TOKEN: &str = "refresh_token";
/// A impressão (sha256) do refresh token que o provedor recusou com `invalid_grant`.
pub const CAMPO_DO_VENCIDO: &str = "vencido";

/// O nome no chaveiro de um campo da conta `id`.
#[must_use]
pub fn nome_da_conta(id: &str, campo: &str) -> String {
    format!("email/{id}/{campo}")
}

/// O nome no chaveiro de um campo do aplicativo do `tipo`.
#[must_use]
pub fn nome_do_app(tipo: TipoDeEmail, campo: &str) -> String {
    format!("email/{}/app/{campo}", tipo.chave())
}

/// As três credenciais.
#[derive(Clone)]
pub struct Credenciais {
    /// O client id (público, mas não vai para log).
    pub client_id: Zeroizing<String>,
    /// O client secret.
    pub client_secret: Zeroizing<String>,
    /// O refresh token.
    pub refresh_token: Zeroizing<String>,
}

impl std::fmt::Debug for Credenciais {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credenciais").finish_non_exhaustive()
    }
}

/// A conta para a qual se procuram credenciais.
#[derive(Debug, Clone, Copy)]
pub struct Alvo<'a> {
    /// O id da conta.
    pub id: &'a str,
    /// O tipo.
    pub tipo: TipoDeEmail,
}

impl<'a> Alvo<'a> {
    /// A conta legada (Gmail, id `gmail`).
    #[must_use]
    pub fn legado() -> Alvo<'static> {
        Alvo {
            id: ID_LEGADO,
            tipo: TipoDeEmail::Gmail,
        }
    }
}

/// O que cada fonte sabe: um campo de cada.
#[derive(Default)]
struct Parcial {
    id: Option<String>,
    segredo: Option<String>,
    token: Option<String>,
}

fn limpo(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn ler_texto(c: &Chaveiro, nome: &str) -> Option<String> {
    if c.tem_segredo_de_mensageiro(nome) {
        limpo(
            c.segredo_de_mensageiro(nome)
                .ok()
                .map(|s| s.expor().to_owned()),
        )
    } else {
        None
    }
}

fn da_conta(c: Option<&Chaveiro>, alvo: Alvo<'_>) -> Parcial {
    let Some(c) = c else {
        return Parcial::default();
    };
    Parcial {
        id: ler_texto(c, &nome_da_conta(alvo.id, CAMPO_DO_ID)),
        segredo: ler_texto(c, &nome_da_conta(alvo.id, CAMPO_DO_SEGREDO)),
        token: ler_texto(c, &nome_da_conta(alvo.id, CAMPO_DO_TOKEN)),
    }
}

fn do_app(c: Option<&Chaveiro>, alvo: Alvo<'_>) -> Parcial {
    let Some(c) = c else {
        return Parcial::default();
    };
    Parcial {
        id: ler_texto(c, &nome_do_app(alvo.tipo, CAMPO_DO_ID)),
        segredo: ler_texto(c, &nome_do_app(alvo.tipo, CAMPO_DO_SEGREDO)),
        token: None,
    }
}

fn do_ambiente_do_processo(variavel: &dyn Fn(&str) -> Option<String>, alvo: Alvo<'_>) -> Parcial {
    if alvo.tipo != TipoDeEmail::Gmail {
        return Parcial::default();
    }
    Parcial {
        id: limpo(variavel(VARIAVEIS[0])),
        segredo: limpo(variavel(VARIAVEIS[1])),
        token: (alvo.id == ID_LEGADO)
            .then(|| limpo(variavel(VARIAVEIS[2])))
            .flatten(),
    }
}

fn dos_env(raizes: &[PathBuf], alvo: Alvo<'_>) -> Parcial {
    let mut p = Parcial::default();
    if alvo.tipo != TipoDeEmail::Gmail {
        return p;
    }
    for raiz in raizes {
        let Ok(texto) = std::fs::read_to_string(raiz.join(".env")) else {
            continue;
        };
        let texto = Zeroizing::new(texto);
        p.id = p.id.or_else(|| limpo(valor_do_env(&texto, VARIAVEIS[0])));
        p.segredo = p
            .segredo
            .or_else(|| limpo(valor_do_env(&texto, VARIAVEIS[1])));
        if alvo.id == ID_LEGADO {
            p.token = p
                .token
                .or_else(|| limpo(valor_do_env(&texto, VARIAVEIS[2])));
        }
    }
    p
}

/// As raízes onde procurar `.env`: o diretório atual e os de cima.
#[must_use]
pub fn raizes_padrao() -> Vec<PathBuf> {
    std::env::current_dir()
        .map(|atual| atual.ancestors().take(6).map(Path::to_path_buf).collect())
        .unwrap_or_default()
}

fn juntar(
    variavel: &dyn Fn(&str) -> Option<String>,
    raizes: &[PathBuf],
    chaveiro: Option<&Chaveiro>,
    alvo: Alvo<'_>,
) -> Parcial {
    let (a, b, c, d) = (
        da_conta(chaveiro, alvo),
        do_app(chaveiro, alvo),
        do_ambiente_do_processo(variavel, alvo),
        dos_env(raizes, alvo),
    );
    Parcial {
        id: a.id.or(b.id).or(c.id).or(d.id),
        segredo: a.segredo.or(b.segredo).or(c.segredo).or(d.segredo),
        token: a.token.or(c.token).or(d.token),
    }
}

/// Os **nomes** das variáveis que nenhuma fonte preenche.
#[must_use]
pub fn faltando_em(
    variavel: &dyn Fn(&str) -> Option<String>,
    raizes: &[PathBuf],
    chaveiro: Option<&Chaveiro>,
    alvo: Alvo<'_>,
) -> Vec<&'static str> {
    let p = juntar(variavel, raizes, chaveiro, alvo);
    [(&p.id, 0), (&p.segredo, 1), (&p.token, 2)]
        .into_iter()
        .filter(|(v, _)| v.is_none())
        .map(|(_, i)| VARIAVEIS[i])
        .collect()
}

/// Os nomes que faltam no ambiente real (processo, `.env` e chaveiro).
#[must_use]
pub fn faltando(chaveiro: Option<&Chaveiro>, alvo: Alvo<'_>) -> Vec<&'static str> {
    faltando_em(&|n| std::env::var(n).ok(), &raizes_padrao(), chaveiro, alvo)
}

/// As três credenciais da conta estão em alguma fonte? (Só sim/não; nunca o valor.)
#[must_use]
pub fn presentes(chaveiro: Option<&Chaveiro>, alvo: Alvo<'_>) -> bool {
    faltando(chaveiro, alvo).is_empty()
}

impl Credenciais {
    /// Procura campo a campo.
    ///
    /// # Errors
    /// [`Erro::SemCredenciais`] com os nomes que faltam.
    pub fn resolver(
        variavel: &dyn Fn(&str) -> Option<String>,
        raizes: &[PathBuf],
        chaveiro: Option<&Chaveiro>,
        alvo: Alvo<'_>,
    ) -> Resultado<Self> {
        let p = juntar(variavel, raizes, chaveiro, alvo);
        match (p.id, p.segredo, p.token) {
            (Some(id), Some(segredo), Some(token)) => Ok(Self {
                client_id: Zeroizing::new(id),
                client_secret: Zeroizing::new(segredo),
                refresh_token: Zeroizing::new(token),
            }),
            (id, segredo, token) => {
                let falta: Vec<&str> = [(id, 0), (segredo, 1), (token, 2)]
                    .into_iter()
                    .filter(|(v, _)| v.is_none())
                    .map(|(_, i)| VARIAVEIS[i])
                    .collect();
                Err(Erro::SemCredenciais(falta.join(", ")))
            }
        }
    }

    /// Do ambiente real.
    ///
    /// # Errors
    /// Como [`Self::resolver`].
    pub fn do_ambiente(chaveiro: Option<&Chaveiro>, alvo: Alvo<'_>) -> Resultado<Self> {
        Self::resolver(&|n| std::env::var(n).ok(), &raizes_padrao(), chaveiro, alvo)
    }

    /// A impressão (sha256 em hex) do refresh token: serve para lembrar qual token falhou sem
    /// guardar o token de novo.
    #[must_use]
    pub fn impressao_do_token(&self) -> String {
        impressao(&self.refresh_token)
    }

    /// O provedor recusou este token e ninguém entrou de novo desde então?
    #[must_use]
    pub fn vencida(&self, chaveiro: Option<&Chaveiro>, id: &str) -> bool {
        token_vencido(chaveiro, id, &self.impressao_do_token())
    }
}

/// O client id e o client secret do aplicativo da conta (sem exigir o refresh token): o que o
/// login precisa.
///
/// # Errors
/// [`Erro::SemCredenciais`] com os nomes que faltam.
pub fn cliente_do_app(
    variavel: &dyn Fn(&str) -> Option<String>,
    raizes: &[PathBuf],
    chaveiro: Option<&Chaveiro>,
    alvo: Alvo<'_>,
) -> Resultado<(Zeroizing<String>, Zeroizing<String>)> {
    let p = juntar(variavel, raizes, chaveiro, alvo);
    match (p.id, p.segredo) {
        (Some(i), Some(s)) => Ok((Zeroizing::new(i), Zeroizing::new(s))),
        (i, s) => {
            let falta: Vec<&str> = [(i, 0), (s, 1)]
                .into_iter()
                .filter(|(v, _)| v.is_none())
                .map(|(_, k)| VARIAVEIS[k])
                .collect();
            Err(Erro::SemCredenciais(falta.join(", ")))
        }
    }
}

/// sha256 em hex.
#[must_use]
pub fn impressao(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn erro_do_chaveiro(e: &impl std::fmt::Display) -> Erro {
    Erro::Chaveiro(e.to_string())
}

fn guardar(c: &Chaveiro, nome: &str, valor: &str) -> Resultado<()> {
    c.guardar_segredo_de_mensageiro(nome, Segredo::novo(valor))
        .map_err(|e| erro_do_chaveiro(&e))
}

fn apagar_nome(c: &Chaveiro, nome: &str) -> Resultado<()> {
    c.apagar_segredo_de_mensageiro(nome)
        .map_err(|e| erro_do_chaveiro(&e))
}

/// O token com esta impressão foi marcado como vencido na conta `id`?
#[must_use]
pub fn token_vencido(chaveiro: Option<&Chaveiro>, id: &str, impressao_do_token: &str) -> bool {
    chaveiro
        .and_then(|c| ler_texto(c, &nome_da_conta(id, CAMPO_DO_VENCIDO)))
        .is_some_and(|marca| marca == impressao_do_token)
}

/// Marca o token como vencido (o provedor respondeu `invalid_grant`). Um token novo (outra
/// impressão) deixa a marca sem efeito.
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn marcar_vencida(chaveiro: &Chaveiro, id: &str, cred: &Credenciais) -> Resultado<()> {
    guardar(
        chaveiro,
        &nome_da_conta(id, CAMPO_DO_VENCIDO),
        &cred.impressao_do_token(),
    )
}

/// Tira a marca de vencido.
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn limpar_vencida(chaveiro: &Chaveiro, id: &str) -> Resultado<()> {
    apagar_nome(chaveiro, &nome_da_conta(id, CAMPO_DO_VENCIDO))
}

/// A conta tem refresh token no chaveiro (de um login ou de uma importação)?
#[must_use]
pub fn tem_token_no_chaveiro(chaveiro: Option<&Chaveiro>, id: &str) -> bool {
    chaveiro.is_some_and(|c| ler_texto(c, &nome_da_conta(id, CAMPO_DO_TOKEN)).is_some())
}

/// O chaveiro tem client id e secret do aplicativo do tipo?
#[must_use]
pub fn tem_app_no_chaveiro(chaveiro: Option<&Chaveiro>, tipo: TipoDeEmail) -> bool {
    chaveiro.is_some_and(|c| {
        ler_texto(c, &nome_do_app(tipo, CAMPO_DO_ID)).is_some()
            && ler_texto(c, &nome_do_app(tipo, CAMPO_DO_SEGREDO)).is_some()
    })
}

/// O chaveiro tem client id próprio da conta (sobrescreve o do aplicativo)?
#[must_use]
pub fn tem_cliente_proprio(chaveiro: Option<&Chaveiro>, id: &str) -> bool {
    chaveiro.is_some_and(|c| ler_texto(c, &nome_da_conta(id, CAMPO_DO_ID)).is_some())
}

/// Guarda o client id e/ou secret **do aplicativo** do tipo (só os preenchidos; vazio não
/// apaga o que já está).
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn guardar_app(
    chaveiro: &Chaveiro,
    tipo: TipoDeEmail,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Resultado<()> {
    for (campo, valor) in [(CAMPO_DO_ID, client_id), (CAMPO_DO_SEGREDO, client_secret)] {
        if let Some(v) = valor.map(str::trim).filter(|v| !v.is_empty()) {
            guardar(chaveiro, &nome_do_app(tipo, campo), v)?;
        }
    }
    Ok(())
}

/// Guarda o client id e/ou secret **próprios da conta** (sobrescrevem os do aplicativo).
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn guardar_cliente_da_conta(
    chaveiro: &Chaveiro,
    id: &str,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Resultado<()> {
    for (campo, valor) in [(CAMPO_DO_ID, client_id), (CAMPO_DO_SEGREDO, client_secret)] {
        if let Some(v) = valor.map(str::trim).filter(|v| !v.is_empty()) {
            guardar(chaveiro, &nome_da_conta(id, campo), v)?;
        }
    }
    Ok(())
}

/// Guarda o resultado de um login: o token novo, e tira a marca de vencido.
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn guardar_login(chaveiro: &Chaveiro, id: &str, refresh_token: &str) -> Resultado<()> {
    guardar(chaveiro, &nome_da_conta(id, CAMPO_DO_TOKEN), refresh_token)?;
    limpar_vencida(chaveiro, id)
}

/// Copia as três do ambiente e do `.env` (sem o chaveiro) para o chaveiro: client id e secret
/// no aplicativo do Gmail e o token na conta legada.
///
/// # Errors
/// [`Erro::SemCredenciais`] se o ambiente não tem as três; [`Erro::Chaveiro`].
pub fn importar(
    variavel: &dyn Fn(&str) -> Option<String>,
    raizes: &[PathBuf],
    chaveiro: &Chaveiro,
) -> Resultado<()> {
    let c = Credenciais::resolver(variavel, raizes, None, Alvo::legado())?;
    guardar_app(
        chaveiro,
        TipoDeEmail::Gmail,
        Some(&c.client_id),
        Some(&c.client_secret),
    )?;
    guardar_login(chaveiro, ID_LEGADO, &c.refresh_token)
}

/// Apaga do chaveiro tudo da conta `id` (não mexe no aplicativo do tipo).
///
/// # Errors
/// [`Erro::Chaveiro`].
pub fn apagar(chaveiro: &Chaveiro, id: &str) -> Resultado<()> {
    for campo in [
        CAMPO_DO_ID,
        CAMPO_DO_SEGREDO,
        CAMPO_DO_TOKEN,
        CAMPO_DO_VENCIDO,
    ] {
        apagar_nome(chaveiro, &nome_da_conta(id, campo))?;
    }
    Ok(())
}

/// O valor de `chave` num texto no formato `.env` (`CHAVE=valor`, `export CHAVE="valor"`,
/// `#` comenta). Só procura a chave pedida.
#[must_use]
pub fn valor_do_env(texto: &str, chave: &str) -> Option<String> {
    for linha in texto.lines() {
        let linha = linha.trim();
        if linha.starts_with('#') {
            continue;
        }
        let linha = linha.strip_prefix("export ").unwrap_or(linha).trim_start();
        let Some((k, v)) = linha.split_once('=') else {
            continue;
        };
        if k.trim() != chave {
            continue;
        }
        let v = v.trim();
        let v = if let Some(entre) = v
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .map(|(a, _)| a)
        {
            entre
        } else if let Some(entre) = v
            .strip_prefix('\'')
            .and_then(|r| r.split_once('\''))
            .map(|(a, _)| a)
        {
            entre
        } else {
            v.split(" #").next().unwrap_or(v).trim()
        };
        return (!v.is_empty()).then(|| v.to_owned());
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use std::sync::Arc;

    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;

    fn chaveiro(pasta: &Path) -> Chaveiro {
        Chaveiro::abrir_com_chaveiro(
            pasta,
            Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap()
    }

    fn ambiente(n: &str) -> Option<String> {
        match n {
            "GOOGLE_MAIL_CLIENT_ID" => Some("id-do-ambiente".to_owned()),
            "GOOGLE_MAIL_CLIENT_SECRET" => Some("segredo-do-ambiente".to_owned()),
            "GMAIL_REFRESH_TOKEN" => Some("token-do-ambiente".to_owned()),
            _ => None,
        }
    }

    fn escrever_env(pasta: &Path, texto: &str) -> Vec<PathBuf> {
        std::fs::write(pasta.join(".env"), texto).unwrap();
        vec![pasta.to_path_buf()]
    }

    const LEGADO: Alvo<'static> = Alvo {
        id: ID_LEGADO,
        tipo: TipoDeEmail::Gmail,
    };
    const OUTRA: Alvo<'static> = Alvo {
        id: "gmail-2",
        tipo: TipoDeEmail::Gmail,
    };

    #[test]
    fn le_o_formato_env_so_da_chave_pedida() {
        let t =
            "# c\nOUTRA=1\nGOOGLE_MAIL_CLIENT_ID=abc\nexport GMAIL_REFRESH_TOKEN=\"t k\"\nVAZIA=\n";
        assert_eq!(
            valor_do_env(t, "GOOGLE_MAIL_CLIENT_ID").as_deref(),
            Some("abc")
        );
        assert_eq!(
            valor_do_env(t, "GMAIL_REFRESH_TOKEN").as_deref(),
            Some("t k")
        );
        assert_eq!(valor_do_env(t, "VAZIA"), None);
        assert_eq!(valor_do_env(t, "NAO_TEM"), None);
    }

    #[test]
    fn ambiente_ganha_do_env_e_falta_da_erro_so_com_nomes() {
        let d = tempfile::tempdir().unwrap();
        let raizes = escrever_env(
            d.path(),
            "GOOGLE_MAIL_CLIENT_ID=id-do-env\nGOOGLE_MAIL_CLIENT_SECRET=segredo-do-env\nGMAIL_REFRESH_TOKEN=token-do-env\n",
        );
        let so_env = Credenciais::resolver(&|_| None, &raizes, None, LEGADO).unwrap();
        assert_eq!(so_env.client_id.as_str(), "id-do-env");
        assert_eq!(so_env.refresh_token.as_str(), "token-do-env");
        let amb = Credenciais::resolver(&ambiente, &raizes, None, LEGADO).unwrap();
        assert_eq!(amb.client_id.as_str(), "id-do-ambiente");
        assert_eq!(amb.client_secret.as_str(), "segredo-do-ambiente");

        let parcial = |n: &str| (n == "GOOGLE_MAIL_CLIENT_ID").then(|| "so-o-id".to_owned());
        let Err(Erro::SemCredenciais(nomes)) = Credenciais::resolver(&parcial, &[], None, LEGADO)
        else {
            panic!("devia faltar");
        };
        assert_eq!(nomes, "GOOGLE_MAIL_CLIENT_SECRET, GMAIL_REFRESH_TOKEN");
        assert!(!nomes.contains("so-o-id"));
    }

    #[test]
    fn o_token_do_ambiente_so_vale_para_a_conta_legada() {
        let outra = Credenciais::resolver(&ambiente, &[], None, OUTRA);
        assert!(matches!(outra, Err(Erro::SemCredenciais(n)) if n == "GMAIL_REFRESH_TOKEN"));
        // O app do ambiente (client id/secret) serve a qualquer conta Gmail.
        let outlook = Alvo {
            id: "outlook",
            tipo: TipoDeEmail::Outlook,
        };
        assert!(Credenciais::resolver(&ambiente, &[], None, outlook).is_err());
    }

    #[test]
    fn o_chaveiro_prevalece_e_a_conta_sobrescreve_o_app() {
        let d = tempfile::tempdir().unwrap();
        let c = chaveiro(d.path());
        guardar_login(&c, ID_LEGADO, "token-novo-do-login").unwrap();
        let cred = Credenciais::resolver(&ambiente, &[], Some(&c), LEGADO).unwrap();
        assert_eq!(cred.refresh_token.as_str(), "token-novo-do-login");
        assert_eq!(cred.client_id.as_str(), "id-do-ambiente");
        // O .env também perde para o chaveiro.
        let raizes = escrever_env(d.path(), "GMAIL_REFRESH_TOKEN=token-vencido-do-env\n");
        let cred = Credenciais::resolver(&|_| None, &raizes, Some(&c), LEGADO);
        assert!(matches!(cred, Err(Erro::SemCredenciais(_))));
        // O app do tipo vale para as duas contas; o da conta sobrescreve.
        guardar_app(
            &c,
            TipoDeEmail::Gmail,
            Some("id-do-app"),
            Some("segredo-do-app"),
        )
        .unwrap();
        guardar_login(&c, "gmail-2", "token-da-dois").unwrap();
        let um = Credenciais::resolver(&ambiente, &[], Some(&c), LEGADO).unwrap();
        let dois = Credenciais::resolver(&ambiente, &[], Some(&c), OUTRA).unwrap();
        assert_eq!(um.client_id.as_str(), "id-do-app");
        assert_eq!(dois.client_id.as_str(), "id-do-app");
        assert_eq!(dois.refresh_token.as_str(), "token-da-dois");
        guardar_cliente_da_conta(&c, "gmail-2", Some("id-proprio"), None).unwrap();
        let dois = Credenciais::resolver(&ambiente, &[], Some(&c), OUTRA).unwrap();
        assert_eq!(dois.client_id.as_str(), "id-proprio");
        assert_eq!(dois.client_secret.as_str(), "segredo-do-app");
        let um = Credenciais::resolver(&ambiente, &[], Some(&c), LEGADO).unwrap();
        assert_eq!(um.client_id.as_str(), "id-do-app");
    }

    #[test]
    fn importar_copia_e_apagar_remove_so_a_conta() {
        let d = tempfile::tempdir().unwrap();
        let c = chaveiro(d.path());
        importar(&ambiente, &[], &c).unwrap();
        assert!(tem_token_no_chaveiro(Some(&c), ID_LEGADO));
        assert!(tem_app_no_chaveiro(Some(&c), TipoDeEmail::Gmail));
        let so_chaveiro = Credenciais::resolver(&|_| None, &[], Some(&c), LEGADO).unwrap();
        assert_eq!(so_chaveiro.client_id.as_str(), "id-do-ambiente");
        assert_eq!(so_chaveiro.refresh_token.as_str(), "token-do-ambiente");
        assert!(importar(&|_| None, &[], &c).is_err());
        apagar(&c, ID_LEGADO).unwrap();
        assert!(!tem_token_no_chaveiro(Some(&c), ID_LEGADO));
        assert!(tem_app_no_chaveiro(Some(&c), TipoDeEmail::Gmail));
        apagar(&c, ID_LEGADO).unwrap(); // idempotente
    }

    #[test]
    fn a_marca_de_vencido_vale_so_para_o_token_que_falhou() {
        let d = tempfile::tempdir().unwrap();
        let c = chaveiro(d.path());
        let velho = Credenciais::resolver(&ambiente, &[], Some(&c), LEGADO).unwrap();
        assert!(!velho.vencida(Some(&c), ID_LEGADO));
        marcar_vencida(&c, ID_LEGADO, &velho).unwrap();
        assert!(velho.vencida(Some(&c), ID_LEGADO));
        assert!(!velho.vencida(Some(&c), "gmail-2"), "por conta");
        guardar_login(&c, ID_LEGADO, "token-novo").unwrap();
        let novo = Credenciais::resolver(&ambiente, &[], Some(&c), LEGADO).unwrap();
        assert_eq!(novo.refresh_token.as_str(), "token-novo");
        assert!(!novo.vencida(Some(&c), ID_LEGADO));
        assert!(!velho.vencida(Some(&c), ID_LEGADO));
    }

    #[test]
    fn debug_nao_mostra_valores() {
        let c = Credenciais::resolver(&ambiente, &[], None, LEGADO).unwrap();
        let t = format!("{c:?}");
        assert!(!t.contains("segredo-do-ambiente") && !t.contains("token-do-ambiente"));
    }
}
