//! Rita sobre o `claude -p` (spec 15, D12): o `claude` roda o laço e chama as ferramentas de
//! Rita por um `sbf mcp --rita` restrito (`--mcp-config`), sem as ferramentas embutidas dele e
//! sem as outras do `sbf mcp`. O teto de 8 chamadas e o "pare no primeiro que passa" são do
//! servidor MCP; `--max-turns` é só a cinta de segurança do próprio `claude`.
//!
//! O prompt (instruções e o texto do documento em bloco de dados) vai pela entrada padrão. O
//! contexto e o estado ficam num arquivo privado (0600, pasta 0700 sob `$XDG_RUNTIME_DIR`),
//! apagados no fim; o texto do documento nunca vai a esse arquivo.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use sbf_ia::{ClaudeCli, ErroDeIa, NaturezaDoCusto, ServidorMcp};

use crate::contexto::DadosDaTela;
use crate::ferramentas::{EstadoDaSessao, FERRAMENTAS, MAXIMO_DE_CHAMADAS};
use crate::laco::{Parada, Relatorio, texto_final};
use crate::mcp::ContextoDoMcp;
use crate::prompt::{instrucoes, pedido_do_usuario};

/// O nome do servidor MCP no `claude`.
pub const NOME_DO_SERVIDOR: &str = "rita";
static CONTADOR: AtomicU64 = AtomicU64::new(0);

/// Rita sobre o `claude -p`.
#[derive(Debug, Clone)]
pub struct ClaudeDeRita {
    cli: ClaudeCli,
    /// O próprio `sbf` (o `claude` o sobe como servidor MCP).
    programa: PathBuf,
    modelo: String,
}

/// Uma pasta privada para o contexto e o estado; apagada ao descartar.
struct PastaPrivada(PathBuf);

impl PastaPrivada {
    fn criar() -> std::io::Result<Self> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let nome = format!(
            "sbf-rita-{}-{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::SeqCst)
        );
        let caminho = base.join(nome);
        let mut b = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            b.mode(0o700);
        }
        b.create(&caminho)?;
        Ok(Self(caminho))
    }
}

impl Drop for PastaPrivada {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn gravar_privado(caminho: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(caminho)?;
        f.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(caminho, bytes)
    }
}

impl ClaudeDeRita {
    /// `programa` é o `sbf` que o `claude` sobe como servidor MCP (`std::env::current_exe()`).
    pub fn novo(cli: ClaudeCli, programa: impl Into<PathBuf>, modelo: impl Into<String>) -> Self {
        Self {
            cli,
            programa: programa.into(),
            modelo: modelo.into(),
        }
    }

    pub fn modelo(&self) -> &str {
        &self.modelo
    }

    /// Um pedido. O `claude` não guarda conversa entre pedidos (`--no-session-persistence`):
    /// cada pedido recomeça, com o contexto da tela.
    ///
    /// # Errors
    /// `claude` ausente ou sem login, limite de uso, prazo, interrupção.
    pub fn pedir(
        &self,
        tela: &DadosDaTela,
        pedido: &str,
        cancelar: &AtomicBool,
    ) -> Result<Relatorio, ErroDeIa> {
        let inicio = Instant::now();
        let pasta = PastaPrivada::criar()
            .map_err(|e| ErroDeIa::Indisponivel(format!("pasta de trabalho: {}", e.kind())))?;
        let arquivo = pasta.0.join("contexto.json");
        let estado = pasta.0.join("estado.json");
        let dados = ContextoDoMcp {
            tela: tela.clone(),
            pedido: pedido.to_owned(),
            modelo: format!("claude-cli {}", self.modelo),
            estado: estado.clone(),
        }
        .sem_texto_do_documento();
        gravar_privado(
            &arquivo,
            &serde_json::to_vec(&dados).map_err(|_| ErroDeIa::Outro("montar o contexto".into()))?,
        )
        .map_err(|e| ErroDeIa::Indisponivel(format!("contexto: {}", e.kind())))?;
        let servidor = ServidorMcp {
            programa: self.programa.clone(),
            argumentos: vec![
                "mcp".to_owned(),
                "--rita".to_owned(),
                "--contexto".to_owned(),
                arquivo.display().to_string(),
            ],
            nome: NOME_DO_SERVIDOR.to_owned(),
        };
        let permitidas: Vec<String> = FERRAMENTAS.iter().map(|f| (*f).to_owned()).collect();
        let prompt = format!("{}\n\n{}", instrucoes(), pedido_do_usuario(tela, pedido));
        let turnos = u32::try_from(MAXIMO_DE_CHAMADAS + 2).unwrap_or(10);
        let saida =
            self.cli
                .conversar_com_mcp(&prompt, &servidor, &permitidas, turnos, cancelar)?;
        let lido: EstadoDaSessao = std::fs::read(&estado)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let parada = if lido.rascunhos.iter().any(|r| r.pre_teste.passou) {
            Parada::Passou
        } else if lido.estourou {
            Parada::Limite
        } else {
            Parada::Respondeu
        };
        let maximo = if lido.maximo == 0 {
            MAXIMO_DE_CHAMADAS
        } else {
            lido.maximo
        };
        let sessao = EstadoDaSessao { maximo, ..lido };
        let melhor = sessao.melhor();
        let resposta = texto_final(parada, &saida.texto, &sessao, melhor);
        Ok(Relatorio {
            resposta,
            parada,
            chamadas: sessao.chamadas,
            rascunhos: sessao.rascunhos,
            melhor,
            consumo: saida.consumo,
            natureza: NaturezaDoCusto::Nominal,
            modelo: saida.modelo,
            duracao: inicio.elapsed(),
        })
    }
}
