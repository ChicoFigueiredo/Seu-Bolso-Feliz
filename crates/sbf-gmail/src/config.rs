//! `email.json`: as contas de e-mail (cada uma, uma Conexão do tipo E-mail) e o que a pessoa
//! escolheu coletar de cada uma (só dado do usuário, nunca segredo: as credenciais ficam no
//! chaveiro), e `email-rotulos-<id>.json`, a última lista de rótulos da conta (para a janela
//! escolher sem rede), tudo na pasta de dados do app.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::erro::{Erro, Resultado};
use crate::fonte::Rotulo;

/// O arquivo da configuração (as contas de e-mail).
pub const ARQUIVO_DA_CONFIG: &str = "email.json";

/// O arquivo da última lista de rótulos da conta `id`.
#[must_use]
pub fn arquivo_dos_rotulos(id: &str) -> String {
    format!("email-rotulos-{id}.json")
}
/// O rótulo seguido enquanto a pessoa não escolheu outros (se existir na conta).
pub const ROTULO_PADRAO: &str = "Comprovantes";

/// O tipo da caixa de e-mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipoDeEmail {
    /// Gmail (OAuth 2.0 do Google).
    Gmail,
    /// Outlook (Microsoft Graph). Ainda **em breve**: o tipo existe no modelo, mas não conecta.
    Outlook,
}

impl TipoDeEmail {
    /// O nome curto, usado em chaves do chaveiro e no CLI.
    #[must_use]
    pub fn chave(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Outlook => "outlook",
        }
    }

    /// O nome para mostrar.
    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::Outlook => "Outlook",
        }
    }

    /// O tipo já conecta de verdade?
    #[must_use]
    pub fn disponivel(self) -> bool {
        matches!(self, Self::Gmail)
    }

    /// Do texto do CLI (`gmail`, `outlook`).
    #[must_use]
    pub fn de_texto(t: &str) -> Option<Self> {
        match t.trim().to_lowercase().as_str() {
            "gmail" => Some(Self::Gmail),
            "outlook" => Some(Self::Outlook),
            _ => None,
        }
    }
}

/// Uma conta de e-mail: uma Conexão do tipo E-mail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conta {
    /// O id estável (`gmail`, `gmail-2`...); nomeia as chaves do chaveiro e a pasta de entrada.
    pub id: String,
    /// O tipo.
    pub tipo: TipoDeEmail,
    /// O nome que a pessoa dá à conta.
    #[serde(default)]
    pub apelido: String,
    /// O e-mail da conta, preenchido depois do login.
    #[serde(default)]
    pub conta: Option<String>,
    /// Os rótulos (ou pastas) seguidos. `None`: a pessoa ainda não escolheu (vale
    /// [`ROTULO_PADRAO`] se a conta o tem). `Some(vazio)`: nenhum, e nada é coletado.
    #[serde(default)]
    pub rotulos: Option<Vec<String>>,
    /// Termos de busca a mais (ex.: `from:banco`), somados a `has:attachment`. Opcional.
    #[serde(default)]
    pub consulta_extra: String,
}

/// O id é válido (minúsculas, dígitos e hífen, de 1 a 24)?
#[must_use]
pub fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 24
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// As contas de e-mail (`email.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// As contas, na ordem em que foram adicionadas.
    pub contas: Vec<Conta>,
}

/// O que a coleta vai seguir numa conta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seguidos {
    /// Estes rótulos (podem estar vazios).
    Rotulos(Vec<String>),
    /// Nada escolhido e a conta não tem o rótulo padrão: a consulta antiga
    /// ([`crate::coletor::CONSULTA_PADRAO`]) na caixa inteira.
    CaixaInteira,
}

fn mesmo_nome(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Config {
    /// Lê `caminho`; ausente dá nenhuma conta.
    ///
    /// # Errors
    /// [`Erro::Argumento`] se o arquivo existe e não é válido.
    pub fn carregar(caminho: &Path) -> Resultado<Self> {
        match std::fs::read_to_string(caminho) {
            Ok(t) => serde_json::from_str(&t)
                .map_err(|_| Erro::Argumento(format!("{ARQUIVO_DA_CONFIG} inválido"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Como [`Self::carregar`], mas sem nenhuma conta e com as credenciais antigas (ambiente
    /// ou `.env`, `GOOGLE_MAIL_*`) à mão, mostra a conta legada `gmail` (em memória; só
    /// `sbf email importar` a grava).
    ///
    /// # Errors
    /// Como [`Self::carregar`].
    pub fn carregar_efetiva(caminho: &Path, legado_disponivel: bool) -> Resultado<Self> {
        let mut c = Self::carregar(caminho)?;
        if c.contas.is_empty() && legado_disponivel {
            c.contas.push(Conta::legada());
        }
        Ok(c)
    }

    /// Grava `caminho` (arquivo temporário e renomeação).
    ///
    /// # Errors
    /// [`Erro::Disco`].
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_vec_pretty(self).map_err(|e| Erro::Argumento(e.to_string()))?;
        escrever_atomico(caminho, &texto)
    }

    /// A conta de id `id`.
    #[must_use]
    pub fn conta(&self, id: &str) -> Option<&Conta> {
        self.contas.iter().find(|c| c.id == id)
    }

    /// A conta de id `id`, para mudar.
    pub fn conta_mut(&mut self, id: &str) -> Option<&mut Conta> {
        self.contas.iter_mut().find(|c| c.id == id)
    }

    /// Adiciona uma conta nova e devolve o id: a primeira do Gmail é `gmail` (a legada, que
    /// mantém a pasta `entrada/gmail/`); as outras, `<tipo>-2`, `<tipo>-3`...
    ///
    /// # Errors
    /// [`Erro::Argumento`] se o tipo ainda não conecta.
    pub fn adicionar(&mut self, tipo: TipoDeEmail, apelido: &str) -> Resultado<String> {
        if !tipo.disponivel() {
            return Err(Erro::Argumento(format!(
                "{} ainda está em breve: só o Gmail conecta por enquanto",
                tipo.rotulo()
            )));
        }
        let base = tipo.chave();
        let id = if tipo == TipoDeEmail::Gmail && self.conta(base).is_none() {
            base.to_owned()
        } else {
            (2..)
                .map(|n| format!("{base}-{n}"))
                .find(|id| self.conta(id).is_none())
                .unwrap_or_else(|| base.to_owned())
        };
        let apelido = apelido.trim();
        self.contas.push(Conta {
            apelido: if apelido.is_empty() {
                tipo.rotulo().to_owned()
            } else {
                apelido.chars().take(40).collect()
            },
            ..Conta::nova(&id, tipo)
        });
        Ok(id)
    }

    /// Remove a conta (só da configuração; o chaveiro e o que já entrou ficam por conta de
    /// quem chama). Devolve `true` se existia.
    pub fn remover(&mut self, id: &str) -> bool {
        let antes = self.contas.len();
        self.contas.retain(|c| c.id != id);
        self.contas.len() != antes
    }
}

impl Conta {
    /// Uma conta vazia.
    #[must_use]
    pub fn nova(id: &str, tipo: TipoDeEmail) -> Self {
        Self {
            id: id.to_owned(),
            tipo,
            apelido: tipo.rotulo().to_owned(),
            conta: None,
            rotulos: None,
            consulta_extra: String::new(),
        }
    }

    /// A conta legada: a que existia antes de haver várias.
    #[must_use]
    pub fn legada() -> Self {
        Self::nova(crate::gravador::ID_LEGADO, TipoDeEmail::Gmail)
    }

    /// O nome para a lista: o e-mail se já se sabe, senão o apelido.
    #[must_use]
    pub fn nome(&self) -> &str {
        self.conta
            .as_deref()
            .filter(|c| !c.is_empty())
            .unwrap_or(&self.apelido)
    }

    /// O que seguir, dados os rótulos que a conta tem (os nomes).
    #[must_use]
    pub fn seguidos(&self, da_conta: &[String]) -> Seguidos {
        match &self.rotulos {
            Some(v) => Seguidos::Rotulos(v.clone()),
            None => {
                if let Some(nome) = da_conta.iter().find(|n| mesmo_nome(n, ROTULO_PADRAO)) {
                    Seguidos::Rotulos(vec![nome.clone()])
                } else {
                    Seguidos::CaixaInteira
                }
            }
        }
    }

    /// Os rótulos seguidos para mostrar (o padrão só aparece se a conta o tem).
    #[must_use]
    pub fn seguidos_para_mostrar(&self, da_conta: &[String]) -> Vec<String> {
        match self.seguidos(da_conta) {
            Seguidos::Rotulos(v) => v,
            Seguidos::CaixaInteira => Vec::new(),
        }
    }

    /// Fixa a lista (tira o "padrão") a partir do que valia.
    fn materializar(&mut self, da_conta: &[String]) -> &mut Vec<String> {
        let atual = self.seguidos_para_mostrar(da_conta);
        self.rotulos.get_or_insert(atual)
    }

    /// O rótulo é seguido?
    #[must_use]
    pub fn segue(&self, nome: &str, da_conta: &[String]) -> bool {
        self.seguidos_para_mostrar(da_conta)
            .iter()
            .any(|n| mesmo_nome(n, nome))
    }

    /// Passa a seguir (idempotente). Devolve `true` se mudou.
    pub fn seguir(&mut self, nome: &str, da_conta: &[String]) -> bool {
        let nome = nome.trim();
        if nome.is_empty() || self.segue(nome, da_conta) {
            return false;
        }
        self.materializar(da_conta).push(nome.to_owned());
        true
    }

    /// Deixa de seguir. Devolve `true` se mudou.
    pub fn deixar(&mut self, nome: &str, da_conta: &[String]) -> bool {
        if !self.segue(nome, da_conta) {
            return false;
        }
        self.materializar(da_conta).retain(|n| !mesmo_nome(n, nome));
        true
    }
}

fn escrever_atomico(caminho: &Path, bytes: &[u8]) -> Resultado<()> {
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, caminho)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct ListaDeRotulos {
    rotulos: Vec<String>,
}

/// Guarda os rótulos **da pessoa** da conta (os de sistema, `INBOX`, `CATEGORY_*`..., ficam
/// de fora: o id deles não começa com `Label_`), ordenados.
///
/// # Errors
/// [`Erro::Disco`].
pub fn guardar_rotulos_da_conta(caminho: &Path, rotulos: &[Rotulo]) -> Resultado<()> {
    let mut nomes: Vec<String> = rotulos
        .iter()
        .filter(|r| r.id.starts_with("Label_"))
        .map(|r| r.nome.clone())
        .collect();
    nomes.sort_by_key(|n| n.to_lowercase());
    nomes.dedup();
    let texto = serde_json::to_vec_pretty(&ListaDeRotulos { rotulos: nomes })
        .map_err(|e| Erro::Argumento(e.to_string()))?;
    escrever_atomico(caminho, &texto)
}

/// A última lista de rótulos da conta (vazia se nunca foi guardada).
#[must_use]
pub fn ler_rotulos_da_conta(caminho: &Path) -> Vec<String> {
    std::fs::read_to_string(caminho)
        .ok()
        .and_then(|t| serde_json::from_str::<ListaDeRotulos>(&t).ok())
        .map(|l| l.rotulos)
        .unwrap_or_default()
}

/// Filtra a lista de rótulos pelo que a pessoa digitou (sem diferenciar maiúsculas), tira os
/// já seguidos e devolve no máximo `limite`.
#[must_use]
pub fn sugestoes<'a>(
    da_conta: &'a [String],
    digitado: &str,
    seguidos: &[String],
    limite: usize,
) -> Vec<&'a str> {
    let d = digitado.trim().to_lowercase();
    da_conta
        .iter()
        .filter(|n| !seguidos.iter().any(|s| mesmo_nome(s, n)))
        .filter(|n| d.is_empty() || n.to_lowercase().contains(&d))
        .take(limite)
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod testes {
    use super::*;

    fn conta() -> Vec<String> {
        ["Comprovantes", "Contas/Luz", "Viagens"]
            .map(str::to_owned)
            .to_vec()
    }

    fn gmail() -> Conta {
        Conta::legada()
    }

    #[test]
    fn padrao_e_comprovantes_se_a_conta_tem() {
        let c = gmail();
        assert_eq!(
            c.seguidos(&conta()),
            Seguidos::Rotulos(vec!["Comprovantes".to_owned()])
        );
        assert_eq!(c.seguidos(&["Viagens".to_owned()]), Seguidos::CaixaInteira);
        assert_eq!(c.seguidos(&[]), Seguidos::CaixaInteira);
    }

    #[test]
    fn seguir_e_deixar_fixam_a_lista_a_partir_do_padrao() {
        let mut c = gmail();
        assert!(c.seguir("viagens", &conta()));
        assert!(!c.seguir("Viagens", &conta()), "idempotente");
        assert_eq!(
            c.rotulos,
            Some(vec!["Comprovantes".to_owned(), "viagens".to_owned()])
        );
        assert!(c.deixar("COMPROVANTES", &conta()));
        assert!(!c.deixar("Comprovantes", &conta()));
        assert_eq!(
            c.seguidos(&conta()),
            Seguidos::Rotulos(vec!["viagens".to_owned()])
        );
        // Tirar o último: nada é coletado (não volta ao padrão).
        assert!(c.deixar("viagens", &conta()));
        assert_eq!(c.seguidos(&conta()), Seguidos::Rotulos(vec![]));
    }

    #[test]
    fn varias_contas_ids_estaveis_e_outlook_em_breve() {
        let mut c = Config::default();
        assert_eq!(c.adicionar(TipoDeEmail::Gmail, "").unwrap(), "gmail");
        assert_eq!(c.adicionar(TipoDeEmail::Gmail, "Casa").unwrap(), "gmail-2");
        assert_eq!(
            c.adicionar(TipoDeEmail::Gmail, "Trabalho").unwrap(),
            "gmail-3"
        );
        assert!(c.remover("gmail-2"));
        assert_eq!(
            c.adicionar(TipoDeEmail::Gmail, "Outra").unwrap(),
            "gmail-2",
            "reaproveita o id livre"
        );
        assert_eq!(c.conta("gmail").unwrap().apelido, "Gmail");
        assert_eq!(c.conta("gmail-3").unwrap().apelido, "Trabalho");
        assert!(c.adicionar(TipoDeEmail::Outlook, "").is_err());
        assert!(!c.remover("nao-existe"));
        assert!(id_valido("gmail-2") && !id_valido("Gmail 2") && !id_valido(""));
        assert_eq!(TipoDeEmail::de_texto(" Gmail "), Some(TipoDeEmail::Gmail));
        assert_eq!(TipoDeEmail::de_texto("x"), None);
    }

    #[test]
    fn conta_legada_so_aparece_sem_nada_gravado_e_com_credenciais_antigas() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join(ARQUIVO_DA_CONFIG);
        assert!(
            Config::carregar_efetiva(&a, false)
                .unwrap()
                .contas
                .is_empty()
        );
        let c = Config::carregar_efetiva(&a, true).unwrap();
        assert_eq!(c.contas, vec![Conta::legada()]);
        assert!(!a.exists(), "só em memória");
        let mut gravada = Config::default();
        gravada.adicionar(TipoDeEmail::Gmail, "X").unwrap();
        gravada.salvar(&a).unwrap();
        assert_eq!(
            Config::carregar_efetiva(&a, true).unwrap().contas[0].apelido,
            "X"
        );
    }

    #[test]
    fn ida_e_volta_no_disco_e_lista_da_conta_sem_rotulos_de_sistema() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join(ARQUIVO_DA_CONFIG);
        assert_eq!(Config::carregar(&a).unwrap(), Config::default());
        let mut c = Config::default();
        c.adicionar(TipoDeEmail::Gmail, "").unwrap();
        c.conta_mut("gmail").unwrap().rotulos = Some(vec!["X".to_owned()]);
        c.salvar(&a).unwrap();
        assert_eq!(Config::carregar(&a).unwrap(), c);
        std::fs::write(&a, "{").unwrap();
        assert!(Config::carregar(&a).is_err());

        let l = d.path().join(arquivo_dos_rotulos("gmail"));
        assert!(ler_rotulos_da_conta(&l).is_empty());
        let r = |id: &str, nome: &str| Rotulo {
            id: id.to_owned(),
            nome: nome.to_owned(),
        };
        guardar_rotulos_da_conta(
            &l,
            &[
                r("INBOX", "INBOX"),
                r("Label_2", "viagens"),
                r("Label_1", "Comprovantes"),
                r("CATEGORY_PROMOTIONS", "CATEGORY_PROMOTIONS"),
            ],
        )
        .unwrap();
        assert_eq!(ler_rotulos_da_conta(&l), ["Comprovantes", "viagens"]);
    }

    #[test]
    fn sugestoes_filtram_pelo_que_foi_digitado_e_limitam() {
        let todas: Vec<String> = (0..20).map(|i| format!("Conta {i}")).collect();
        assert_eq!(sugestoes(&todas, "", &[], 8).len(), 8);
        assert_eq!(sugestoes(&todas, "conta 1", &[], 50).len(), 11);
        assert_eq!(
            sugestoes(&conta(), "VIA", &[], 8),
            ["Viagens"],
            "sem diferenciar maiúsculas"
        );
        assert!(sugestoes(&conta(), "viagens", &["Viagens".to_owned()], 8).is_empty());
    }
}
