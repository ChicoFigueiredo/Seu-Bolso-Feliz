//! Os provedores de Rita (spec 15, D12): uma lista configurável, com chaves e endereços no
//! **Chaveiro**. Local (Ollama, `qwen3-coder:30b`), `claude -p` (assinatura), OpenRouter, API da
//! Anthropic e qualquer API compatível com OpenAI (endereço, modelo, chave).
//!
//! A configuração (tipo, endereço, modelo) é um texto JSON guardado como segredo `ia:rita-config`
//! e cada chave fica em `ia:rita/<id>`: nada disso entra em arquivo do cofre, log ou erro. A
//! exceção é o OpenRouter, que reutiliza a chave `ia:openrouter` de Opções → IA (uma chave só,
//! sem segunda entrada); uma `ia:rita/openrouter` de versões anteriores é migrada para ela.
//!
//! OpenRouter e Anthropic têm endereço fixo (`openrouter.ai`, `api.anthropic.com`): a chave
//! nunca vai para outro host por causa de uma configuração.

use std::sync::{Arc, Mutex, PoisonError};

use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_ia::{
    Anthropic, ChatCompletions, ClaudeCli, ConfiguracaoAnthropic, ConfiguracaoChatCompletions,
    ENDERECO_DA_ANTHROPIC, ENDERECO_DO_OLLAMA, ENDERECO_DO_OPENROUTER, ErroDeIa,
    MODELO_LOCAL_PADRAO, ProvedorDeConversa,
};
use serde::{Deserialize, Serialize};

use crate::claude::ClaudeDeRita;

const CHAVE_DA_CONFIGURACAO: &str = "rita-config";
/// O id da chave do OpenRouter no Chaveiro, o mesmo de Opções → IA (`ia:openrouter`).
pub const CHAVE_DO_OPENROUTER: &str = "openrouter";

/// O tipo de um provedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeProvedor {
    /// Ollama (ou outro servidor local no formato OpenAI), sem custo.
    Local,
    /// O `claude -p`, na assinatura.
    ClaudeCli,
    OpenRouter,
    /// A API Messages da Anthropic.
    Anthropic,
    /// Qualquer API compatível com OpenAI.
    Compativel,
}

impl TipoDeProvedor {
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Local => "Local (Ollama)",
            Self::ClaudeCli => "claude -p (assinatura)",
            Self::OpenRouter => "OpenRouter",
            Self::Anthropic => "API da Anthropic",
            Self::Compativel => "Compatível com OpenAI",
        }
    }

    /// Este tipo usa chave guardada.
    pub fn usa_chave(self) -> bool {
        matches!(self, Self::OpenRouter | Self::Anthropic | Self::Compativel)
    }

    /// Este tipo pede um endereço.
    pub fn pede_endereco(self) -> bool {
        matches!(self, Self::Local | Self::Compativel)
    }

    /// O endereço fixo do tipo (OpenRouter e Anthropic): a chave só vai para este host.
    pub fn endereco_fixo(self) -> Option<&'static str> {
        match self {
            Self::OpenRouter => Some(ENDERECO_DO_OPENROUTER),
            Self::Anthropic => Some(ENDERECO_DA_ANTHROPIC),
            _ => None,
        }
    }

    pub const TODOS: [Self; 5] = [
        Self::Local,
        Self::ClaudeCli,
        Self::OpenRouter,
        Self::Anthropic,
        Self::Compativel,
    ];
}

/// Um provedor configurado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvedorConfigurado {
    /// Identificador estável (letras minúsculas, dígitos, hífen); nome da chave no Chaveiro.
    pub id: String,
    pub nome: String,
    pub tipo: TipoDeProvedor,
    pub endereco: Option<String>,
    pub modelo: String,
}

impl ProvedorConfigurado {
    /// O id da chave deste provedor na [`GuardaDeProvedores`]: o OpenRouter usa sempre a chave
    /// de Opções → IA (`openrouter`); os outros, o próprio id.
    pub fn id_da_chave(&self) -> &str {
        if self.tipo == TipoDeProvedor::OpenRouter {
            CHAVE_DO_OPENROUTER
        } else {
            &self.id
        }
    }

    /// Os valores de fábrica de cada tipo.
    pub fn padrao_do_tipo(tipo: TipoDeProvedor) -> Self {
        let (id, nome, endereco, modelo) = match tipo {
            TipoDeProvedor::Local => (
                "local",
                "Local",
                Some(ENDERECO_DO_OLLAMA),
                MODELO_LOCAL_PADRAO,
            ),
            TipoDeProvedor::ClaudeCli => ("claude-cli", "claude -p", None, "sonnet"),
            TipoDeProvedor::OpenRouter => (
                "openrouter",
                "OpenRouter",
                Some(ENDERECO_DO_OPENROUTER),
                "anthropic/claude-sonnet-4.5",
            ),
            TipoDeProvedor::Anthropic => (
                "anthropic",
                "API da Anthropic",
                Some(ENDERECO_DA_ANTHROPIC),
                "claude-sonnet-4-5",
            ),
            TipoDeProvedor::Compativel => ("compativel", "Compatível", Some("https://"), ""),
        };
        Self {
            id: id.to_owned(),
            nome: nome.to_owned(),
            tipo,
            endereco: endereco.map(str::to_owned),
            modelo: modelo.to_owned(),
        }
    }
}

/// A lista de provedores e o principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConfiguracaoDeRita {
    pub provedores: Vec<ProvedorConfigurado>,
    /// O id do provedor principal (o que o painel usa se o usuário não trocar).
    pub padrao: Option<String>,
}

impl ConfiguracaoDeRita {
    /// Os provedores que existem antes de o usuário configurar qualquer coisa: o local e o
    /// `claude -p`.
    pub fn inicial() -> Self {
        Self {
            provedores: vec![
                ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::Local),
                ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::ClaudeCli),
            ],
            padrao: Some("local".to_owned()),
        }
    }

    pub fn por_id(&self, id: &str) -> Option<&ProvedorConfigurado> {
        self.provedores.iter().find(|p| p.id == id)
    }

    /// O provedor principal (ou o primeiro).
    pub fn o_padrao(&self) -> Option<&ProvedorConfigurado> {
        self.padrao
            .as_deref()
            .and_then(|id| self.por_id(id))
            .or_else(|| self.provedores.first())
    }

    /// Confere a lista: ids únicos e válidos, modelo e, quando pedido, endereço.
    ///
    /// # Errors
    /// O que está errado, em português.
    pub fn conferir(&self) -> Result<(), String> {
        for (i, p) in self.provedores.iter().enumerate() {
            if !id_valido(&p.id) {
                return Err(format!("identificador inválido `{}`", p.id));
            }
            if self.provedores[..i].iter().any(|o| o.id == p.id) {
                return Err(format!("identificador repetido `{}`", p.id));
            }
            if p.modelo.trim().is_empty() {
                return Err(format!("{}: falta o modelo", p.nome));
            }
            if p.id == CHAVE_DO_OPENROUTER && p.tipo != TipoDeProvedor::OpenRouter {
                return Err(format!(
                    "{}: o identificador `{CHAVE_DO_OPENROUTER}` é do OpenRouter",
                    p.nome
                ));
            }
            if p.tipo.pede_endereco() {
                // A mesma leitura estrita com que o provedor abre a conexão: o prefixo
                // `http://localhost` não basta (`http://localhost.exemplo.com` é outro host).
                sbf_ia::validar_endereco(p.endereco.as_deref().unwrap_or("")).map_err(|_| {
                    format!(
                        "{}: o endereço precisa ser https (http só em localhost)",
                        p.nome
                    )
                })?;
            }
            if let Some(fixo) = p.tipo.endereco_fixo()
                && p.endereco
                    .as_deref()
                    .map(|e| e.trim().trim_end_matches('/'))
                    .is_some_and(|e| !e.is_empty() && e != fixo)
            {
                return Err(format!(
                    "{}: o endereço de {} é fixo ({fixo}) e não muda",
                    p.nome,
                    p.tipo.rotulo()
                ));
            }
        }
        Ok(())
    }
}

/// `true` para ids como `meu-servidor-2`.
pub fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Onde a configuração e as chaves moram: o Chaveiro de verdade, ou a memória (demonstração e
/// testes).
pub trait GuardaDeProvedores: Send + Sync {
    fn ler(&self) -> ConfiguracaoDeRita;
    /// # Errors
    /// Texto para a tela (nunca o valor).
    fn gravar(&self, c: &ConfiguracaoDeRita) -> Result<(), String>;
    fn chave(&self, id: &str) -> Option<Segredo>;
    /// Há chave guardada, **sem decifrá-la** (o Chaveiro só consulta os rótulos).
    fn tem_chave(&self, id: &str) -> bool {
        self.chave(id).is_some()
    }
    /// Os 4 últimos caracteres da chave guardada (`Some("")` se curta), ou `None` sem chave.
    fn final_da_chave(&self, id: &str) -> Option<String> {
        self.chave(id).map(|c| {
            let v = c.expor();
            if v.chars().count() < 12 {
                String::new()
            } else {
                v.chars().skip(v.chars().count() - 4).collect()
            }
        })
    }
    /// # Errors
    /// Texto para a tela.
    fn guardar_chave(&self, id: &str, chave: Segredo) -> Result<(), String>;
    /// # Errors
    /// Texto para a tela.
    fn apagar_chave(&self, id: &str) -> Result<(), String>;
}

/// O rótulo (sem o prefixo `ia:`) de uma chave de provedor: `openrouter` é a chave de Opções →
/// IA; os outros, `rita/<id>`.
fn chave_do_provedor(id: &str) -> String {
    if id == CHAVE_DO_OPENROUTER {
        CHAVE_DO_OPENROUTER.to_owned()
    } else {
        format!("rita/{id}")
    }
}

/// O rótulo antigo da chave do OpenRouter de Rita (antes de ela usar a de Opções → IA).
const CHAVE_ANTIGA_DO_OPENROUTER: &str = "rita/openrouter";

/// Migração simples: se existe `ia:rita/openrouter` e ainda não existe `ia:openrouter`, a chave
/// passa para `ia:openrouter` e a antiga é apagada. Com as duas guardadas, não mexe em nada (a
/// de Opções → IA vale; a antiga fica até o usuário apagar). Devolve `true` se migrou.
pub fn migrar_chave_do_openrouter(c: &Chaveiro) -> bool {
    if c.tem_chave_de_ia(CHAVE_DO_OPENROUTER) || !c.tem_chave_de_ia(CHAVE_ANTIGA_DO_OPENROUTER) {
        return false;
    }
    let Ok(antiga) = c.chave_de_ia(CHAVE_ANTIGA_DO_OPENROUTER) else {
        return false;
    };
    if c.guardar_chave_de_ia(CHAVE_DO_OPENROUTER, antiga).is_err() {
        return false;
    }
    let _ = c.apagar_chave_de_ia(CHAVE_ANTIGA_DO_OPENROUTER);
    true
}

impl GuardaDeProvedores for Chaveiro {
    fn ler(&self) -> ConfiguracaoDeRita {
        self.chave_de_ia(CHAVE_DA_CONFIGURACAO)
            .ok()
            .and_then(|s| serde_json::from_str(s.expor()).ok())
            .unwrap_or_else(ConfiguracaoDeRita::inicial)
    }

    fn gravar(&self, c: &ConfiguracaoDeRita) -> Result<(), String> {
        c.conferir()?;
        let json = serde_json::to_string(c)
            .map_err(|_| "não deu para montar a configuração".to_owned())?;
        self.guardar_chave_de_ia(CHAVE_DA_CONFIGURACAO, Segredo::novo(json))
            .map_err(|e| e.to_string())
    }

    fn chave(&self, id: &str) -> Option<Segredo> {
        if id == CHAVE_DO_OPENROUTER {
            migrar_chave_do_openrouter(self);
        }
        self.chave_de_ia(&chave_do_provedor(id)).ok()
    }

    fn tem_chave(&self, id: &str) -> bool {
        if id == CHAVE_DO_OPENROUTER {
            migrar_chave_do_openrouter(self);
        }
        self.tem_chave_de_ia(&chave_do_provedor(id))
    }

    fn guardar_chave(&self, id: &str, chave: Segredo) -> Result<(), String> {
        self.guardar_chave_de_ia(&chave_do_provedor(id), chave)
            .map_err(|e| e.to_string())
    }

    fn apagar_chave(&self, id: &str) -> Result<(), String> {
        self.apagar_chave_de_ia(&chave_do_provedor(id))
            .map_err(|e| e.to_string())
    }
}

/// Uma guarda em memória.
#[derive(Debug, Default)]
pub struct GuardaEmMemoria {
    config: Mutex<Option<ConfiguracaoDeRita>>,
    chaves: Mutex<Vec<(String, String)>>,
}

impl GuardaDeProvedores for GuardaEmMemoria {
    fn ler(&self) -> ConfiguracaoDeRita {
        self.config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(ConfiguracaoDeRita::inicial)
    }

    fn gravar(&self, c: &ConfiguracaoDeRita) -> Result<(), String> {
        c.conferir()?;
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = Some(c.clone());
        Ok(())
    }

    fn chave(&self, id: &str) -> Option<Segredo> {
        self.chaves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, c)| Segredo::novo(c.clone()))
    }

    fn guardar_chave(&self, id: &str, chave: Segredo) -> Result<(), String> {
        let mut v = self.chaves.lock().unwrap_or_else(PoisonError::into_inner);
        v.retain(|(i, _)| i != id);
        v.push((id.to_owned(), chave.expor().to_owned()));
        Ok(())
    }

    fn apagar_chave(&self, id: &str) -> Result<(), String> {
        self.chaves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(i, _)| i != id);
        Ok(())
    }
}

/// Uma guarda com a configuração em memória: o painel pergunta a cada quadro e o Chaveiro
/// (D-Bus, arquivo cifrado) só é lido na primeira vez e depois de uma gravação. As chaves
/// passam direto (quem as mostra guarda o resultado, como a aba IA).
pub struct ComCache {
    por_baixo: Arc<dyn GuardaDeProvedores>,
    config: Mutex<Option<ConfiguracaoDeRita>>,
}

impl std::fmt::Debug for ComCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComCache").finish_non_exhaustive()
    }
}

impl ComCache {
    pub fn nova(por_baixo: Arc<dyn GuardaDeProvedores>) -> Self {
        Self {
            por_baixo,
            config: Mutex::new(None),
        }
    }
}

impl GuardaDeProvedores for ComCache {
    fn ler(&self) -> ConfiguracaoDeRita {
        let mut c = self.config.lock().unwrap_or_else(PoisonError::into_inner);
        c.get_or_insert_with(|| self.por_baixo.ler()).clone()
    }

    fn gravar(&self, c: &ConfiguracaoDeRita) -> Result<(), String> {
        self.por_baixo.gravar(c)?;
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = Some(c.clone());
        Ok(())
    }

    fn chave(&self, id: &str) -> Option<Segredo> {
        self.por_baixo.chave(id)
    }

    fn tem_chave(&self, id: &str) -> bool {
        self.por_baixo.tem_chave(id)
    }

    fn final_da_chave(&self, id: &str) -> Option<String> {
        self.por_baixo.final_da_chave(id)
    }

    fn guardar_chave(&self, id: &str, chave: Segredo) -> Result<(), String> {
        self.por_baixo.guardar_chave(id, chave)
    }

    fn apagar_chave(&self, id: &str) -> Result<(), String> {
        self.por_baixo.apagar_chave(id)
    }
}

/// Um provedor pronto para usar.
pub enum ProvedorDaRita {
    /// Rita roda o laço e o provedor responde um passo por vez.
    Conversa(Arc<dyn ProvedorDeConversa>),
    /// O `claude -p` roda o laço e chama as ferramentas por MCP.
    Claude(ClaudeDeRita),
}

impl std::fmt::Debug for ProvedorDaRita {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conversa(p) => f.debug_tuple("Conversa").field(&p.nome()).finish(),
            Self::Claude(_) => f.write_str("Claude"),
        }
    }
}

/// Monta o provedor `p`. **Roda na thread do pedido, nunca na da interface**: procura o
/// `claude` no disco e consulta o Chaveiro. A chave só é lida a cada chamada de rede.
///
/// OpenRouter e Anthropic usam o endereço fixo (`openrouter.ai`, `api.anthropic.com`),
/// ignorando o da configuração: a chave nunca segue para outro host.
///
/// # Errors
/// [`ErroDeIa::Indisponivel`]: endereço inválido, `claude` não encontrado, chave que falta.
pub fn construir(
    p: &ProvedorConfigurado,
    guarda: &Arc<dyn GuardaDeProvedores>,
    programa_sbf: &std::path::Path,
) -> Result<ProvedorDaRita, ErroDeIa> {
    let fonte = |id: String| -> Arc<dyn sbf_ia::FonteDaChave> {
        let g = Arc::clone(guarda);
        Arc::new(move || g.chave(&id))
    };
    let endereco = p.endereco.clone().unwrap_or_default();
    match p.tipo {
        TipoDeProvedor::Local => {
            let c = ConfiguracaoChatCompletions::local(endereco, p.modelo.clone());
            Ok(ProvedorDaRita::Conversa(Arc::new(ChatCompletions::novo(
                c, None,
            )?)))
        }
        TipoDeProvedor::OpenRouter => {
            let c = ConfiguracaoChatCompletions::openrouter(p.modelo.clone());
            Ok(ProvedorDaRita::Conversa(Arc::new(ChatCompletions::novo(
                c,
                Some(fonte(p.id_da_chave().to_owned())),
            )?)))
        }
        TipoDeProvedor::Compativel => {
            let c =
                ConfiguracaoChatCompletions::compativel(p.id.clone(), endereco, p.modelo.clone());
            // Com ou sem chave: se há uma guardada, vai; se não, o endereço é de uso livre.
            let com_chave = guarda.tem_chave(&p.id).then(|| fonte(p.id.clone()));
            Ok(ProvedorDaRita::Conversa(Arc::new(ChatCompletions::novo(
                c, com_chave,
            )?)))
        }
        TipoDeProvedor::Anthropic => {
            let c = ConfiguracaoAnthropic::nova(p.modelo.clone());
            Ok(ProvedorDaRita::Conversa(Arc::new(Anthropic::novo(
                c,
                fonte(p.id.clone()),
            )?)))
        }
        TipoDeProvedor::ClaudeCli => {
            let cli = ClaudeCli::do_sistema()
                .or_else(claude_do_usuario)
                .ok_or_else(|| ErroDeIa::Indisponivel("o `claude` não foi encontrado".to_owned()))?
                .com_modelos(p.modelo.clone(), p.modelo.clone())
                .com_prazo(std::time::Duration::from_secs(600));
            Ok(ProvedorDaRita::Claude(ClaudeDeRita::novo(
                cli,
                programa_sbf,
                p.modelo.clone(),
            )))
        }
    }
}

/// `~/.local/bin/claude`, onde o instalador oficial o põe.
fn claude_do_usuario() -> Option<ClaudeCli> {
    let casa = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let caminho = casa.join(".local/bin/claude");
    caminho.is_file().then(|| ClaudeCli::novo(caminho))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_configuracao_inicial_tem_local_e_claude() {
        let c = ConfiguracaoDeRita::inicial();
        assert_eq!(c.provedores.len(), 2);
        assert_eq!(
            c.o_padrao().map(|p| p.modelo.as_str()),
            Some("qwen3-coder:30b")
        );
        assert!(c.conferir().is_ok());
    }

    #[test]
    fn conferir_recusa_id_repetido_modelo_vazio_e_http_fora_de_localhost() {
        let mut c = ConfiguracaoDeRita::inicial();
        c.provedores.push(c.provedores[0].clone());
        assert!(c.conferir().unwrap_err().contains("repetido"));
        let mut c = ConfiguracaoDeRita::inicial();
        c.provedores[0].modelo = String::new();
        assert!(c.conferir().is_err());
        let mut c = ConfiguracaoDeRita::inicial();
        c.provedores[0].endereco = Some("http://exemplo.com/v1".into());
        assert!(c.conferir().unwrap_err().contains("https"));
    }

    #[test]
    fn a_guarda_em_memoria_guarda_chave_e_mostra_so_o_final() {
        let g = GuardaEmMemoria::default();
        assert!(g.final_da_chave("x").is_none());
        g.guardar_chave("x", Segredo::novo("sk-ficticia-1234567890"))
            .expect("guardar");
        assert_eq!(g.final_da_chave("x").as_deref(), Some("7890"));
        g.apagar_chave("x").expect("apagar");
        assert!(g.chave("x").is_none());
    }

    #[test]
    fn o_cache_so_le_de_baixo_uma_vez_e_atualiza_ao_gravar() {
        let baixo = Arc::new(GuardaEmMemoria::default());
        let cache = ComCache::nova(baixo.clone());
        assert_eq!(cache.ler().provedores.len(), 2);
        let mut c = ConfiguracaoDeRita::inicial();
        c.provedores.pop();
        baixo.gravar(&c).expect("gravar por baixo");
        // Mudou por baixo, mas o cache ainda tem a leitura de antes.
        assert_eq!(cache.ler().provedores.len(), 2);
        cache.gravar(&c).expect("gravar");
        assert_eq!(cache.ler().provedores.len(), 1);
    }

    #[test]
    fn construir_local_e_compativel_nao_chama_a_rede() {
        let guarda: Arc<dyn GuardaDeProvedores> = Arc::new(GuardaEmMemoria::default());
        let local = ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::Local);
        assert!(construir(&local, &guarda, std::path::Path::new("/usr/bin/sbf")).is_ok());
        let mut comp = ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::Compativel);
        comp.endereco = Some("https://api.exemplo.com/v1".into());
        comp.modelo = "m".into();
        assert!(construir(&comp, &guarda, std::path::Path::new("/usr/bin/sbf")).is_ok());
        comp.endereco = Some("http://exemplo.com/v1".into());
        assert!(construir(&comp, &guarda, std::path::Path::new("/usr/bin/sbf")).is_err());
    }

    #[test]
    fn conferir_usa_a_leitura_estrita_e_fixa_o_endereco_de_openrouter_e_anthropic() {
        for ruim in [
            "http://localhost.evil.com/v1",
            "http://localhost@evil.com/v1",
            "http://127.0.0.1.evil.com",
        ] {
            let mut c = ConfiguracaoDeRita::inicial();
            c.provedores[0].endereco = Some(ruim.into());
            assert!(c.conferir().unwrap_err().contains("https"), "{ruim}");
        }
        for bom in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:11434/v1",
        ] {
            let mut c = ConfiguracaoDeRita::inicial();
            c.provedores[0].endereco = Some(bom.into());
            assert!(c.conferir().is_ok(), "{bom}");
        }
        for tipo in [TipoDeProvedor::OpenRouter, TipoDeProvedor::Anthropic] {
            let mut c = ConfiguracaoDeRita::inicial();
            let mut p = ProvedorConfigurado::padrao_do_tipo(tipo);
            assert!(
                {
                    c.provedores.push(p.clone());
                    c.conferir().is_ok()
                },
                "o endereço de fábrica vale"
            );
            c.provedores.pop();
            p.endereco = Some("https://coletor.exemplo.com/v1".into());
            c.provedores.push(p.clone());
            assert!(c.conferir().unwrap_err().contains("fixo"), "{tipo:?}");
            c.provedores.pop();
            p.endereco = None;
            c.provedores.push(p);
            assert!(c.conferir().is_ok(), "sem endereço vale: o fixo é usado");
        }
    }

    #[test]
    fn so_o_openrouter_pode_ter_o_id_openrouter() {
        let mut c = ConfiguracaoDeRita::inicial();
        let mut p = ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::Compativel);
        p.id = "openrouter".into();
        p.endereco = Some("https://api.exemplo.com/v1".into());
        p.modelo = "m".into();
        c.provedores.push(p);
        assert!(c.conferir().unwrap_err().contains("OpenRouter"));
    }

    #[test]
    fn construir_openrouter_e_anthropic_ignoram_o_endereco_da_configuracao() {
        let guarda: Arc<dyn GuardaDeProvedores> = Arc::new(GuardaEmMemoria::default());
        for (tipo, fixo) in [
            (TipoDeProvedor::OpenRouter, "https://openrouter.ai/api/v1"),
            (TipoDeProvedor::Anthropic, "https://api.anthropic.com/v1"),
        ] {
            let mut p = ProvedorConfigurado::padrao_do_tipo(tipo);
            p.endereco = Some("https://coletor.exemplo.com/v1".into());
            let ProvedorDaRita::Conversa(c) =
                construir(&p, &guarda, std::path::Path::new("/usr/bin/sbf")).expect("provedor")
            else {
                panic!("devia ser conversa");
            };
            assert_eq!(c.endereco(), fixo, "{tipo:?}");
        }
    }

    fn chaveiro() -> (tempfile::TempDir, Chaveiro) {
        let pasta = tempfile::tempdir().expect("pasta");
        let c = Chaveiro::abrir_com_chaveiro(
            pasta.path(),
            Arc::new(bancada_segredos::testes::ChaveiroEmMemoria::novo()),
            bancada_segredos::ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .expect("chaveiro");
        (pasta, c)
    }

    #[test]
    fn a_chave_do_openrouter_de_rita_e_a_de_opcoes_sem_segunda_entrada() {
        let (_pasta, c) = chaveiro();
        let p = ProvedorConfigurado::padrao_do_tipo(TipoDeProvedor::OpenRouter);
        assert_eq!(p.id_da_chave(), "openrouter");
        // A chave de Opções → IA vale para Rita, sem cópia.
        c.guardar_chave_de_ia("openrouter", Segredo::novo("sk-or-v1-ficticia-0000abcd"))
            .expect("guardar");
        assert!(GuardaDeProvedores::tem_chave(&c, p.id_da_chave()));
        assert_eq!(
            GuardaDeProvedores::final_da_chave(&c, p.id_da_chave()).as_deref(),
            Some("abcd")
        );
        // Guardar pela guarda de Rita troca a mesma entrada.
        GuardaDeProvedores::guardar_chave(
            &c,
            "openrouter",
            Segredo::novo("sk-or-v1-ficticia-9999wxyz"),
        )
        .expect("trocar");
        assert!(!c.tem_chave_de_ia("rita/openrouter"));
        assert_eq!(
            c.chave_de_ia("openrouter").expect("chave").expor(),
            "sk-or-v1-ficticia-9999wxyz"
        );
        // Outro provedor com chave segue em `rita/<id>`.
        GuardaDeProvedores::guardar_chave(&c, "anthropic", Segredo::novo("sk-ant-ficticia-1234"))
            .expect("guardar");
        assert!(c.tem_chave_de_ia("rita/anthropic"));
    }

    #[test]
    fn a_chave_antiga_de_rita_migra_para_a_de_opcoes() {
        let (_pasta, c) = chaveiro();
        c.guardar_chave_de_ia("rita/openrouter", Segredo::novo("sk-or-v1-antiga-0000abcd"))
            .expect("guardar");
        assert!(!c.tem_chave_de_ia("openrouter"));
        assert!(
            GuardaDeProvedores::tem_chave(&c, "openrouter"),
            "migra ao consultar"
        );
        assert!(!c.tem_chave_de_ia("rita/openrouter"), "a antiga some");
        assert_eq!(
            c.chave_de_ia("openrouter").expect("chave").expor(),
            "sk-or-v1-antiga-0000abcd"
        );
        assert!(
            !migrar_chave_do_openrouter(&c),
            "depois de migrar não há o que migrar"
        );
    }

    #[test]
    fn com_as_duas_chaves_a_de_opcoes_vale_e_nada_e_apagado() {
        let (_pasta, c) = chaveiro();
        c.guardar_chave_de_ia("openrouter", Segredo::novo("sk-or-v1-nova-0000abcd"))
            .expect("guardar");
        c.guardar_chave_de_ia("rita/openrouter", Segredo::novo("sk-or-v1-velha-0000wxyz"))
            .expect("guardar");
        assert!(!migrar_chave_do_openrouter(&c));
        assert_eq!(
            GuardaDeProvedores::chave(&c, "openrouter")
                .expect("chave")
                .expor(),
            "sk-or-v1-nova-0000abcd"
        );
        assert!(c.tem_chave_de_ia("rita/openrouter"));
    }
}
