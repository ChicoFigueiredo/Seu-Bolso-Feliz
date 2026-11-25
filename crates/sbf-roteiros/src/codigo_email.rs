//! O código do MFA por e-mail, lido do Gmail por uma busca estreita (contrato, item 6).
//!
//! Só serve a roteiro cujo manifesto declara `movimenta_dinheiro: false`; essa regra é do
//! importador, não daqui. O remetente e o formato do código vêm do MANIFESTO, nunca do
//! script Luau. O fluxo:
//!
//! 1. o executor marca `pedido_em` (o instante em que o site mandou o código);
//! 2. [`esperar_codigo`] consulta a [`CaixaDeEmail`] em intervalos, até o prazo;
//! 3. só vale mensagem recebida **depois** de `pedido_em` (nunca um código velho), e o formato
//!    é aplicado ancorado (`\b(?:formato)\b`): exatamente um código por mensagem;
//! 4. o código sai como [`Segredo`] (zeroize) e vai direto ao campo; nunca a log, `Debug`,
//!    erro nem ao script.
//!
//! **Autorização:** o adaptador [`CaixaGmail`] reaproveita o `sbf-gmail`: o mesmo refresh token
//! do chaveiro, o mesmo cliente HTTP. A consulta é estreita: `from:<remetente> after:<instante>`,
//! no máximo [`MAXIMO_DE_MENSAGENS`] mensagens, sem baixar anexo nem olhar outra caixa.
//!
//! O tempo é injetável ([`Relogio`]): nos testes não há `sleep` de verdade.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};
use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_coletor::gmail::mensagem::MensagemDoGmail;
use sbf_gmail::corpo::texto_do_corpo;
use sbf_gmail::fonte::{ClienteReal, FonteDeGmail};
use sbf_gmail::{Alvo, Erro as ErroDoGmail};
use zeroize::{Zeroize, Zeroizing};

/// A janela máxima de espera pelo código.
pub const JANELA_MAXIMA: Duration = Duration::from_secs(300);
/// A janela padrão de espera pelo código.
pub const JANELA_PADRAO: Duration = Duration::from_secs(120);
/// O intervalo padrão entre duas consultas.
pub const INTERVALO_PADRAO: Duration = Duration::from_secs(5);
/// Quantas mensagens, no máximo, o adaptador do Gmail lê por consulta.
pub const MAXIMO_DE_MENSAGENS: usize = 5;
/// O trecho do corpo (em caracteres) que interessa: o código está sempre no começo.
const TRECHO_DO_CORPO: usize = 2000;
/// O maior formato aceito (em caracteres).
const MAIOR_FORMATO: usize = 64;

/// Por que o código não veio. Nenhuma variante carrega corpo, assunto, remetente nem código.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeCodigo {
    /// O prazo acabou sem chegar mensagem nova do remetente.
    #[error("o código não chegou por e-mail dentro do prazo")]
    Expirou,
    /// Chegou mensagem nova do remetente, mas nenhuma tem um código no formato esperado.
    #[error("chegou e-mail do remetente, mas sem código no formato esperado")]
    SemCodigo,
    /// Há mais de um código diferente na mesma mensagem (ou no mesmo instante): não dá para
    /// saber qual vale.
    #[error("há mais de um código diferente no e-mail; não dá para saber qual vale")]
    Ambiguo,
    /// A caixa de e-mail não respondeu (rede, serviço fora do ar).
    #[error("não consegui consultar a caixa de e-mail")]
    CaixaIndisponivel,
    /// A autorização do e-mail falta ou venceu: é preciso entrar de novo na conta.
    #[error("a autorização do e-mail falta ou venceu: entre de novo em Conexões › E-mail")]
    Autorizacao,
    /// O remetente ou o formato do manifesto não servem.
    #[error("o remetente ou o formato do código no manifesto é inválido")]
    Invalido,
}

/// A hora e a espera, para os testes não dormirem.
pub trait Relogio {
    /// O instante de agora.
    fn agora(&self) -> SystemTime;
    /// Espera `duracao`.
    fn esperar(&self, duracao: Duration);
}

/// O relógio de verdade.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelogioReal;

impl Relogio for RelogioReal {
    fn agora(&self) -> SystemTime {
        SystemTime::now()
    }

    fn esperar(&self, duracao: Duration) {
        std::thread::sleep(duracao);
    }
}

/// O mínimo de uma mensagem para achar o código: o assunto e o começo do corpo, num só texto.
/// O texto é zerado ao descartar; o `Debug` não o mostra.
pub struct MensagemCurta {
    /// Quando a mensagem chegou.
    pub recebida_em: SystemTime,
    /// Assunto, quebra de linha e o começo do corpo.
    pub corpo: Zeroizing<String>,
}

impl std::fmt::Debug for MensagemCurta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MensagemCurta").finish_non_exhaustive()
    }
}

/// A caixa de e-mail onde o código chega.
pub trait CaixaDeEmail {
    /// As mensagens do `remetente` recebidas a partir de `desde`, no máximo
    /// [`MAXIMO_DE_MENSAGENS`], em qualquer ordem. Só do remetente pedido.
    ///
    /// # Errors
    /// [`ErroDeCodigo::CaixaIndisponivel`] ou [`ErroDeCodigo::Autorizacao`].
    fn mensagens_recentes(
        &self,
        remetente: &str,
        desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo>;
}

/// O que procurar e por quanto tempo. O `Debug` não mostra o remetente.
#[derive(Clone)]
pub struct BuscaDeCodigo {
    remetente: String,
    formato: Regex,
    /// Quanto tempo esperar o código, no máximo [`JANELA_MAXIMA`].
    pub janela: Duration,
    /// O intervalo entre duas consultas.
    pub intervalo: Duration,
}

impl std::fmt::Debug for BuscaDeCodigo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuscaDeCodigo")
            .field("janela", &self.janela)
            .field("intervalo", &self.intervalo)
            .finish_non_exhaustive()
    }
}

/// O remetente é um endereço (`a@b.test`) ou um domínio (`b.test`), só com caracteres seguros
/// para a consulta do Gmail (nada de espaço, aspas, parêntese ou chave).
#[must_use]
pub fn remetente_valido(remetente: &str) -> bool {
    !remetente.is_empty()
        && remetente.len() <= 254
        && remetente
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+'))
        && remetente.matches('@').count() <= 1
}

impl BuscaDeCodigo {
    /// Uma busca com a janela e o intervalo padrões.
    ///
    /// `formato` é uma expressão regular (ex.: `\d{6}`); a busca a ancora em limites de palavra.
    ///
    /// # Errors
    /// [`ErroDeCodigo::Invalido`] se o remetente tem caractere fora do endereço, se o formato
    /// não compila, é grande demais ou casa o texto vazio.
    pub fn nova(remetente: &str, formato: &str) -> Result<Self, ErroDeCodigo> {
        if !remetente_valido(remetente)
            || formato.is_empty()
            || formato.chars().count() > MAIOR_FORMATO
        {
            return Err(ErroDeCodigo::Invalido);
        }
        // O formato sozinho tem de compilar: evita `1)|(2` escapar do grupo que o ancora.
        let sozinho = RegexBuilder::new(formato)
            .size_limit(100_000)
            .build()
            .map_err(|_| ErroDeCodigo::Invalido)?;
        if sozinho.is_match("") {
            return Err(ErroDeCodigo::Invalido);
        }
        let ancorado = RegexBuilder::new(&format!(r"\b(?:{formato})\b"))
            .size_limit(100_000)
            .build()
            .map_err(|_| ErroDeCodigo::Invalido)?;
        Ok(Self {
            remetente: remetente.to_owned(),
            formato: ancorado,
            janela: JANELA_PADRAO,
            intervalo: INTERVALO_PADRAO,
        })
    }

    /// Troca a janela de espera (no máximo [`JANELA_MAXIMA`]; o excesso é cortado).
    #[must_use]
    pub fn com_janela(mut self, janela: Duration) -> Self {
        self.janela = janela.min(JANELA_MAXIMA);
        self
    }

    /// Troca o intervalo entre consultas (pelo menos 1 ms, para o laço sempre andar).
    #[must_use]
    pub fn com_intervalo(mut self, intervalo: Duration) -> Self {
        self.intervalo = intervalo.max(Duration::from_millis(1));
        self
    }

    /// O remetente buscado.
    #[must_use]
    pub fn remetente(&self) -> &str {
        &self.remetente
    }
}

/// O código único da mensagem: `Ok(None)` se não há, `Err(Ambiguo)` se há mais de um diferente.
fn codigo_da_mensagem(
    formato: &Regex,
    texto: &str,
) -> Result<Option<Zeroizing<String>>, ErroDeCodigo> {
    let mut achado: Option<Zeroizing<String>> = None;
    for m in formato.find_iter(texto) {
        match &achado {
            None => achado = Some(Zeroizing::new(m.as_str().to_owned())),
            Some(a) if a.as_str() != m.as_str() => return Err(ErroDeCodigo::Ambiguo),
            Some(_) => {}
        }
    }
    Ok(achado)
}

/// O código mais novo entre as mensagens recebidas depois de `pedido_em`.
///
/// Devolve `Ok(None)` se não há código, e diz em `viu_recente` se havia mensagem nova.
fn procurar(
    mensagens: &[MensagemCurta],
    busca: &BuscaDeCodigo,
    pedido_em: SystemTime,
    viu_recente: &mut bool,
) -> Result<Option<Zeroizing<String>>, ErroDeCodigo> {
    let mut melhor: Option<(SystemTime, Zeroizing<String>)> = None;
    for m in mensagens.iter().filter(|m| m.recebida_em > pedido_em) {
        *viu_recente = true;
        let Some(codigo) = codigo_da_mensagem(&busca.formato, &m.corpo)? else {
            continue;
        };
        match &melhor {
            Some((quando, _)) if m.recebida_em < *quando => {}
            // Duas mensagens no mesmo instante com códigos diferentes: sem como escolher.
            Some((quando, atual)) if m.recebida_em == *quando => {
                if atual.as_str() != codigo.as_str() {
                    return Err(ErroDeCodigo::Ambiguo);
                }
            }
            _ => melhor = Some((m.recebida_em, codigo)),
        }
    }
    Ok(melhor.map(|(_, c)| c))
}

/// Espera o código do MFA chegar por e-mail e o devolve como [`Segredo`].
///
/// Consulta a `caixa` a cada `busca.intervalo` até `busca.janela` (no máximo 300 s) e só olha
/// mensagem recebida **depois** de `pedido_em`. Se a mensagem mais nova com código tem dois
/// códigos diferentes, é [`ErroDeCodigo::Ambiguo`]; com mais de uma mensagem, vale a mais nova.
/// Falha transitória da caixa é tentada de novo até o prazo; falha de autorização para na hora.
///
/// # Errors
/// [`ErroDeCodigo::Expirou`], [`ErroDeCodigo::SemCodigo`], [`ErroDeCodigo::Ambiguo`],
/// [`ErroDeCodigo::CaixaIndisponivel`] e [`ErroDeCodigo::Autorizacao`].
pub fn esperar_codigo(
    caixa: &dyn CaixaDeEmail,
    busca: &BuscaDeCodigo,
    pedido_em: SystemTime,
    relogio: &dyn Relogio,
) -> Result<Segredo, ErroDeCodigo> {
    let janela = busca.janela.min(JANELA_MAXIMA);
    let prazo = relogio.agora() + janela;
    let mut viu_recente = false;
    let mut consultou = false;
    loop {
        match caixa.mensagens_recentes(&busca.remetente, pedido_em) {
            Ok(mensagens) => {
                consultou = true;
                if let Some(codigo) = procurar(&mensagens, busca, pedido_em, &mut viu_recente)? {
                    return Ok(Segredo::novo(codigo.as_str()));
                }
            }
            Err(ErroDeCodigo::CaixaIndisponivel) => {}
            Err(outro) => return Err(outro),
        }
        let agora = relogio.agora();
        if agora >= prazo {
            return Err(if viu_recente {
                ErroDeCodigo::SemCodigo
            } else if consultou {
                ErroDeCodigo::Expirou
            } else {
                ErroDeCodigo::CaixaIndisponivel
            });
        }
        let falta = prazo.duration_since(agora).unwrap_or_default();
        relogio.esperar(busca.intervalo.min(falta));
    }
}

/// A caixa de e-mail sobre o Gmail do `sbf-gmail`: o mesmo cliente e a mesma autorização.
pub struct CaixaGmail<F: FonteDeGmail> {
    fonte: F,
}

impl<F: FonteDeGmail> std::fmt::Debug for CaixaGmail<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaixaGmail").finish_non_exhaustive()
    }
}

impl CaixaGmail<ClienteReal> {
    /// O Gmail real da conta `alvo`, com as credenciais do chaveiro (as mesmas da coleta).
    /// Não chame de dentro de um runtime do tokio.
    ///
    /// # Errors
    /// [`ErroDeCodigo::Autorizacao`] se faltam credenciais; [`ErroDeCodigo::CaixaIndisponivel`]
    /// se o cliente não subiu.
    pub fn real(chaveiro: Option<&Chaveiro>, alvo: Alvo<'_>) -> Result<Self, ErroDeCodigo> {
        ClienteReal::resolver(chaveiro, alvo)
            .map(Self::com_fonte)
            .map_err(do_gmail)
    }
}

impl<F: FonteDeGmail> CaixaGmail<F> {
    /// Uma caixa sobre qualquer fonte do Gmail (os testes usam uma gravada).
    #[must_use]
    pub fn com_fonte(fonte: F) -> Self {
        Self { fonte }
    }

    /// A fonte do Gmail por baixo (os testes conferem a consulta que ela recebeu).
    #[must_use]
    pub fn fonte(&self) -> &F {
        &self.fonte
    }
}

fn do_gmail(e: ErroDoGmail) -> ErroDeCodigo {
    match e {
        ErroDoGmail::Autorizacao | ErroDoGmail::SemCredenciais(_) | ErroDoGmail::SemContas => {
            ErroDeCodigo::Autorizacao
        }
        _ => ErroDeCodigo::CaixaIndisponivel,
    }
}

/// O endereço de um cabeçalho `From` (`Fulano <a@b.test>` ou `a@b.test`), em minúsculas.
///
/// `None` se o cabeçalho é ambíguo: com vírgula (uma lista de remetentes, ou um nome que tenta
/// esconder outro endereço) ou com mais de um `<` (`<a@b.test> <c@d.test>`). Na dúvida, a
/// mensagem não vale: o código do MFA só vem de um remetente inequívoco.
fn endereco_do_cabecalho(de: &str) -> Option<String> {
    if de.contains(',') || de.matches('<').count() > 1 || de.matches('>').count() > 1 {
        return None;
    }
    let miolo = match (de.find('<'), de.find('>')) {
        (Some(a), Some(b)) if a < b => &de[a + 1..b],
        (None, None) => de,
        _ => return None,
    };
    Some(miolo.trim().to_lowercase())
}

/// O cabeçalho `De` é mesmo do remetente? Endereço (`a@b.test`): igual. Domínio (`b.test`): o
/// domínio do endereço é ele ou um subdomínio dele.
fn do_remetente(de: &str, remetente: &str) -> bool {
    let Some(endereco) = endereco_do_cabecalho(de) else {
        return false;
    };
    let remetente = remetente.to_lowercase();
    if remetente.contains('@') {
        return endereco == remetente;
    }
    endereco.rsplit_once('@').is_some_and(|(_, dominio)| {
        dominio == remetente || dominio.ends_with(&format!(".{remetente}"))
    })
}

fn cabecalho<'a>(m: &'a MensagemDoGmail, nome: &str) -> &'a str {
    m.payload
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(nome))
        .map_or("", |h| h.value.as_str())
}

/// Quantos cabeçalhos com este nome a mensagem tem.
fn quantos_cabecalhos(m: &MensagemDoGmail, nome: &str) -> usize {
    m.payload
        .headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case(nome))
        .count()
}

/// Algum cabeçalho `Authentication-Results` diz que a mensagem **falhou** no DMARC ou no SPF?
/// Sem o cabeçalho, nada se afirma (nem todo provedor o grava); com `dmarc=fail` ou `spf=fail`
/// a mensagem é de quem fingiu ser o remetente e é descartada.
fn reprovada_na_autenticacao(m: &MensagemDoGmail) -> bool {
    m.payload
        .headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("Authentication-Results"))
        .any(|h| {
            let compacto: String = h
                .value
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_lowercase();
            compacto.contains("dmarc=fail") || compacto.contains("spf=fail")
        })
}

/// A consulta estreita: o remetente e o instante (segundos desde a época, como o Gmail pede).
fn consulta(remetente: &str, desde: SystemTime) -> String {
    let segundos = desde
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("from:{remetente} after:{segundos}")
}

impl<F: FonteDeGmail> CaixaDeEmail for CaixaGmail<F> {
    fn mensagens_recentes(
        &self,
        remetente: &str,
        desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
        if !remetente_valido(remetente) {
            return Err(ErroDeCodigo::Invalido);
        }
        let pagina = self
            .fonte
            .listar(None, &consulta(remetente, desde), None)
            .map_err(do_gmail)?;
        let mut saida = Vec::new();
        for referencia in pagina.messages.iter().take(MAXIMO_DE_MENSAGENS) {
            let mensagem = self.fonte.mensagem(&referencia.id).map_err(do_gmail)?;
            // O Gmail casa `from:` de forma frouxa: confere o endereço de verdade, exige um
            // único `From` e descarta quem falhou no DMARC ou no SPF.
            if quantos_cabecalhos(&mensagem, "From") != 1
                || !do_remetente(cabecalho(&mensagem, "From"), remetente)
                || reprovada_na_autenticacao(&mensagem)
            {
                continue;
            }
            // Sem data interna legível não há como provar que o código é novo: descarta.
            let Some(ms) = mensagem.internal_date.trim().parse::<u64>().ok() else {
                continue;
            };
            let mut texto = Zeroizing::new(String::new());
            texto.push_str(cabecalho(&mensagem, "Subject"));
            texto.push('\n');
            let mut corpo = texto_do_corpo(&mensagem);
            texto.extend(corpo.chars().take(TRECHO_DO_CORPO));
            corpo.zeroize();
            saida.push(MensagemCurta {
                recebida_em: UNIX_EPOCH + Duration::from_millis(ms),
                corpo: texto,
            });
        }
        Ok(saida)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const REMETENTE: &str = "nao-responda@portal.exemplo.test";

    #[test]
    fn o_formato_e_ancorado_em_limites_de_palavra() {
        let b = BuscaDeCodigo::nova(REMETENTE, r"[A-Z]{2}-\d{4}").expect("busca");
        let achado = |t: &str| codigo_da_mensagem(&b.formato, t).expect("sem ambiguidade");
        assert_eq!(
            achado("código AB-1234 válido")
                .as_deref()
                .map(String::as_str),
            Some("AB-1234")
        );
        assert!(achado("XAB-1234Z").is_none());
        assert!(achado("sem nada").is_none());
    }

    #[test]
    fn gmail_com_dominio_como_remetente_aceita_subdominio_e_nada_mais() {
        assert!(do_remetente(
            "a@mail.portal.exemplo.test",
            "portal.exemplo.test"
        ));
        assert!(do_remetente(
            "Fulano <a@portal.exemplo.test>",
            "portal.exemplo.test"
        ));
        assert!(!do_remetente(
            "a@malportal.exemplo.test",
            "portal.exemplo.test"
        ));
        assert!(!do_remetente(
            "a@portal.exemplo.test.mal.test",
            "portal.exemplo.test"
        ));
        assert!(!do_remetente("a@outro.test", "a@portal.exemplo.test"));
    }

    #[test]
    fn from_ambiguo_nao_vale() {
        let r = "nao-responda@portal.exemplo.test";
        assert!(do_remetente("Portal <nao-responda@portal.exemplo.test>", r));
        assert!(do_remetente("nao-responda@portal.exemplo.test", r));
        for ruim in [
            // lista de remetentes: o "primeiro" é o bom, o resto engana
            "nao-responda@portal.exemplo.test, atacante@mal.test",
            "atacante@mal.test, <nao-responda@portal.exemplo.test>",
            "\"Portal, <nao-responda@portal.exemplo.test>\" <atacante@mal.test>",
            // mais de um `<`
            "<atacante@mal.test> <nao-responda@portal.exemplo.test>",
            "Portal <nao-responda@portal.exemplo.test> <atacante@mal.test>",
            // ângulos desencontrados
            "Portal nao-responda@portal.exemplo.test>",
            "Portal >nao-responda@portal.exemplo.test<",
        ] {
            assert!(!do_remetente(ruim, r), "{ruim}");
            assert!(endereco_do_cabecalho(ruim).is_none(), "{ruim}");
        }
    }
}
