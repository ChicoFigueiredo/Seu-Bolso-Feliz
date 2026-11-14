//! A cola do RAG local e dos pontos de atenção (spec 10): monta, sobre o `sbf-indice` e o
//! `sbf-pontos`, o que o `sbf worker` e a janela usam.
//!
//! O modelo de embeddings (FastEmbed) só entra na compilação com `--features modelo-real`
//! (ele puxa o ONNX Runtime). Sem a feature, o índice não indexa e a área Perguntar explica
//! por quê. **O download do modelo só acontece em [`ServicoReal::baixar_modelo`]**, que a
//! janela chama depois de avisar o usuário; o worker e o MCP nunca baixam.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_ia::NivelDeConfianca;
use sbf_indice::fontes::atualizar;
use sbf_indice::perguntar::{K_PADRAO, Resposta, ServicoDePerguntar, perguntar};
use sbf_indice::{BuscaLocal, Embedder, Indice};
use sbf_livro::Data;
use sbf_pontos::{Armazem, Contexto, Limites};
use sbf_triagem::servico::{ProvedorEscolhido, configuracao_salva};
use sbf_worker::laco::{Analista, Dados, Indexador, ResumoDoIndice, ResumoDosPontos};
use sbf_worker::registro::Falha;

use crate::triagem::{Insumos, Motor, montar_motor};

/// O nível de confiança do destinatário de Perguntar: o do motor de IA escolhido, que como
/// todo destinatário novo nasce mascarado (ADR-015). A tela de Destinatários (fatia futura)
/// é quem vai deixar mudar.
pub const NIVEL_DE_PERGUNTAR: NivelDeConfianca = NivelDeConfianca::Mascarado;

#[cfg(feature = "modelo-real")]
/// `modelos/`, na pasta de dados do app (fora do cofre).
#[must_use]
pub fn pasta_do_modelo(pasta_de_dados: &Path) -> PathBuf {
    pasta_de_dados.join("modelos")
}

/// `indice.sqlite`.
#[must_use]
pub fn caminho_do_indice(pasta_de_dados: &Path) -> PathBuf {
    pasta_de_dados.join("indice.sqlite")
}

/// `pontos.sqlite`.
#[must_use]
pub fn caminho_dos_pontos(pasta_de_dados: &Path) -> PathBuf {
    pasta_de_dados.join("pontos.sqlite")
}

/// O cache do livro-caixa mais recente da estação, se houver.
fn livro_da_estacao() -> Option<PathBuf> {
    let pasta = sbf_estacao::pasta_de_dados_padrao().ok()?;
    let raiz = pasta.join(sbf_estacao::SUBPASTA_DO_LIVRO_CAIXA);
    std::fs::read_dir(raiz)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join("livro-caixa.sqlite"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max()
        .map(|(_, p)| p)
}

/// O modelo já está baixado?
#[must_use]
pub fn modelo_pronto(pasta_de_dados: &Path) -> bool {
    #[cfg(feature = "modelo-real")]
    {
        sbf_indice::modelo_baixado(&pasta_do_modelo(pasta_de_dados))
    }
    #[cfg(not(feature = "modelo-real"))]
    {
        let _ = pasta_de_dados;
        false
    }
}

/// Carrega o embedder. `baixar = false`: só se o modelo já está na pasta (worker e MCP).
/// `baixar = true`: baixa se faltar (só a janela, depois do aviso).
fn carregar_embedder(pasta_de_dados: &Path, baixar: bool) -> Result<Arc<dyn Embedder>, String> {
    #[cfg(feature = "modelo-real")]
    {
        if !baixar && !modelo_pronto(pasta_de_dados) {
            return Err("o modelo de busca ainda não foi baixado".to_owned());
        }
        let e = sbf_indice::FastEmbedder::novo(pasta_do_modelo(pasta_de_dados))
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(e))
    }
    #[cfg(not(feature = "modelo-real"))]
    {
        let _ = (pasta_de_dados, baixar);
        Err(
            "esta compilação do app não inclui o modelo de busca local (compile com \
             `--features modelo-real`)"
                .to_owned(),
        )
    }
}

/// A busca por semelhança para a triagem e o MCP: `None` se o modelo não está pronto ou o
/// índice ainda não existe.
#[must_use]
pub fn busca_local(pasta_de_dados: &Path) -> Option<BuscaLocal> {
    let indice = caminho_do_indice(pasta_de_dados);
    if !indice.is_file() {
        return None;
    }
    let e = carregar_embedder(pasta_de_dados, false).ok()?;
    Some(BuscaLocal::nova(indice, e))
}

/// O índice atualizado de 30 em 30 minutos pelo `sbf worker`. Nunca baixa o modelo.
pub struct IndexadorReal {
    dados: Dados,
    embedder: Option<Arc<dyn Embedder>>,
}

impl IndexadorReal {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self {
            dados,
            embedder: None,
        }
    }
}

impl Indexador for IndexadorReal {
    fn atualizar(&mut self, _cancelar: &AtomicBool) -> Result<Option<ResumoDoIndice>, Falha> {
        if self.embedder.is_none() {
            if !modelo_pronto(&self.dados.pasta) {
                return Ok(None);
            }
            self.embedder =
                Some(carregar_embedder(&self.dados.pasta, false).map_err(|_| Falha::Externo)?);
        }
        let Some(e) = self.embedder.as_ref() else {
            return Ok(None);
        };
        let inv = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        let mut indice =
            Indice::abrir(caminho_do_indice(&self.dados.pasta)).map_err(|_| Falha::Disco)?;
        let livro = livro_da_estacao();
        let r =
            atualizar(&mut indice, &inv, livro.as_deref(), e.as_ref()).map_err(|_| Falha::Banco)?;
        Ok(Some(ResumoDoIndice {
            embutidos: r.embutidos,
            mantidos: r.mantidos,
            removidos: r.removidos,
        }))
    }
}

/// A análise dos pontos de atenção de 6 em 6 horas, no `sbf worker`.
pub struct AnalistaReal {
    dados: Dados,
}

impl AnalistaReal {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self { dados }
    }
}

/// A análise inteira, com o que a estação sabe: disco do cofre, reserva de IA, cache do livro.
///
/// # Errors
/// Falha do SQLite ou de disco.
pub fn analisar_pontos(
    cofre: &Path,
    banco: &Path,
    pasta_de_dados: &Path,
) -> Result<sbf_pontos::Resumo, String> {
    let inv = sbf_worker::banco::abrir(banco).map_err(|e| e.to_string())?;
    let livro = livro_da_estacao();
    let cifra = sbf_disco::cifra_do_volume(cofre);
    let disco_cifrado = match cifra {
        sbf_disco::Cifra::Cifrado => Some(true),
        sbf_disco::Cifra::NaoCifrado => Some(false),
        sbf_disco::Cifra::Desconhecido => None,
    };
    let config = configuracao_salva(banco);
    let reserva = config
        .reserva
        .filter(|r| *r != config.principal)
        .map(|r| match r {
            ProvedorEscolhido::ClaudeCli => "claude-cli",
            ProvedorEscolhido::OpenRouter => "openrouter",
        });
    let mut ctx = Contexto::novo(&inv, Data::hoje_em_sao_paulo());
    ctx.cache_do_livro = livro.as_deref();
    ctx.disco_cifrado = disco_cifrado;
    ctx.provedor_reserva = reserva;
    let mut armazem =
        Armazem::abrir(caminho_dos_pontos(pasta_de_dados)).map_err(|e| e.to_string())?;
    sbf_pontos::rodar(
        &mut armazem,
        &ctx,
        &Limites::default(),
        &sbf_ia::agora_rfc3339(),
    )
    .map_err(|e| e.to_string())
}

impl Analista for AnalistaReal {
    fn analisar(&mut self) -> Result<ResumoDosPontos, Falha> {
        let r = analisar_pontos(&self.dados.cofre, &self.dados.banco, &self.dados.pasta)
            .map_err(|_| Falha::Banco)?;
        Ok(ResumoDosPontos {
            novos: r.novos,
            reabertos: r.reabertos,
            fechados: r.fechados_sozinhos,
        })
    }
}

/// O serviço de Perguntar da janela: índice local + o motor de IA de Opções → IA.
pub struct ServicoReal {
    dados: Dados,
    insumos: Insumos,
    embedder: Option<Arc<dyn Embedder>>,
    motor: Option<Motor>,
}

impl ServicoReal {
    #[must_use]
    pub fn novo(dados: Dados, insumos: Insumos) -> Self {
        Self {
            dados,
            insumos,
            embedder: None,
            motor: None,
        }
    }
}

impl ServicoDePerguntar for ServicoReal {
    fn perguntar(&mut self, pergunta: &str) -> Result<Resposta, String> {
        if self.embedder.is_none() {
            self.embedder = Some(carregar_embedder(&self.dados.pasta, false)?);
        }
        if self.motor.is_none() {
            self.motor = Some(montar_motor(&self.dados.banco, &self.insumos, false)?);
        }
        let (Some(e), Some(m)) = (self.embedder.as_ref(), self.motor.as_ref()) else {
            return Err("o serviço não ficou pronto".to_owned());
        };
        let busca = BuscaLocal::nova(caminho_do_indice(&self.dados.pasta), Arc::clone(e));
        perguntar(
            &busca,
            m.principal.as_ref(),
            m.registro.as_ref(),
            NIVEL_DE_PERGUNTAR,
            pergunta,
            K_PADRAO,
        )
        .map_err(|e| e.to_string())
    }

    fn modelo_pronto(&self) -> bool {
        modelo_pronto(&self.dados.pasta)
    }

    fn baixar_modelo(&mut self) -> Result<(), String> {
        self.embedder = Some(carregar_embedder(&self.dados.pasta, true)?);
        Ok(())
    }

    fn trechos_no_indice(&self) -> Option<usize> {
        Indice::abrir(caminho_do_indice(&self.dados.pasta))
            .ok()?
            .total()
            .ok()
    }
}
