//! A triagem por IA do inventário de verdade (spec 09): monta a [`Fila`] com o provedor
//! principal (o `claude` em modo headless) e a reserva (OpenRouter, só com chave no chaveiro
//! e ligada em Opções → IA) e entrega o [`TriagemDoInventario`] à janela.
//!
//! O que esta montagem **não** faz: chamar provedor nenhum. Quem chama é a rodada, quando o
//! usuário clica em Iniciar, ou o teste de conexão, quando clica em "Testar conexão".
//!
//! # O `claude` e a regra de processo externo
//!
//! `sbf_processo::comando` só roda um programa por **caminho absoluto** que seja
//! `confiavel`: arquivo comum que nem ele nem a pasta dele (depois de seguir links) podem ser
//! escritos por "outros", nem pelo grupo, a menos que o grupo seja o do root ou o grupo
//! próprio do dono. **Não exige dono.** O `~/.local/bin/claude` da instalação padrão é um
//! link para `~/.local/share/claude/versions/<v>`; com `umask 022` ou o grupo próprio do
//! usuário (Ubuntu e Fedora) ele passa. Se a pasta tiver escrita de grupo para um grupo
//! compartilhado, o `claude` é recusado e o teste de conexão diz que o provedor está
//! indisponível: corrija com `chmod g-w`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sbf_chaveiro::Chaveiro;
use sbf_ia::{
    ClaudeCli, ConfiguracaoOpenRouter, ErroDeIa, Fila, FonteDaChave, OpenRouter, Pedido, Provedor,
    Registro, Resposta, ResultadoDeIa,
};
use sbf_inventario::sugestao::RegistroDaIa;
use sbf_triagem::rodada::{RastroDeFalhas, configuracao_da_fila, configuracao_da_fila_v2};
use sbf_triagem::servico::{ProvedorEscolhido, TriagemDoInventario, configuracao_salva};
use sbf_triagem::sugestoes::gasto_do_mes_dezmilesimos;

/// O nome da chave da OpenRouter no chaveiro (`ia:openrouter`).
pub const PROVEDOR_DA_CHAVE: &str = "openrouter";

/// O `claude` por um caminho absoluto de confiança (a busca única fica em `sbf-ia`).
pub use sbf_ia::resolver_claude;

/// Um provedor que não está disponível: o teste de conexão e a rodada mostram o motivo.
#[derive(Debug)]
struct SemProvedor {
    motivo: String,
    nome: &'static str,
}

impl Provedor for SemProvedor {
    fn nome(&self) -> &'static str {
        self.nome
    }
    fn pedir(&self, _pedido: &Pedido) -> ResultadoDeIa<Resposta> {
        Err(ErroDeIa::Indisponivel(self.motivo.clone()))
    }
}

/// O que a montagem precisa de fora: o `claude` achado e a chave do OpenRouter.
#[derive(Clone, Default)]
pub struct Insumos {
    pub claude: Option<PathBuf>,
    /// `Some` só se há chave guardada no chaveiro.
    pub chave_do_openrouter: Option<Arc<dyn FonteDaChave>>,
    /// Onde o `claude` cria a pasta de trabalho (padrão `$XDG_RUNTIME_DIR`).
    pub pasta_de_trabalho: Option<PathBuf>,
}

impl Insumos {
    /// Do ambiente: o `claude` do usuário e a chave do chaveiro (se houver).
    pub fn do_ambiente(chaveiro: Option<Chaveiro>) -> Self {
        let chave_do_openrouter = chaveiro
            .filter(|c| c.tem_chave_de_ia(PROVEDOR_DA_CHAVE))
            .map(|c| {
                Arc::new(move || c.chave_de_ia(PROVEDOR_DA_CHAVE).ok()) as Arc<dyn FonteDaChave>
            });
        Self {
            claude: resolver_claude(),
            chave_do_openrouter,
            pasta_de_trabalho: None,
        }
    }
}

fn claude(insumos: &Insumos) -> Arc<dyn Provedor> {
    match &insumos.claude {
        Some(caminho) => {
            let mut c = ClaudeCli::novo(caminho);
            if let Some(p) = &insumos.pasta_de_trabalho {
                c = c.com_pasta_base(p);
            }
            Arc::new(c)
        }
        None => Arc::new(SemProvedor {
            motivo: "o Claude Code (`claude`) não foi encontrado em uma pasta de confiança"
                .to_owned(),
            nome: "claude-cli",
        }),
    }
}

/// O OpenRouter com os tetos de Opções → IA e o gasto do mês restaurado de `saida_ia`.
fn openrouter(
    banco: &Path,
    fonte: &Arc<dyn FonteDaChave>,
    teto_da_rodada_centavos: u32,
    teto_do_mes_centavos: u32,
) -> Result<Arc<dyn Provedor>, String> {
    // Um centavo de dólar são 100 dezmilésimos.
    let config = ConfiguracaoOpenRouter {
        teto_rodada_dezmilesimos: u64::from(teto_da_rodada_centavos) * 100,
        teto_mes_dezmilesimos: u64::from(teto_do_mes_centavos) * 100,
        ..ConfiguracaoOpenRouter::default()
    };
    let o = OpenRouter::novo(Arc::clone(fonte), config).map_err(|e| e.to_string())?;
    let gasto = gasto_do_mes_dezmilesimos(banco, "openrouter").map_err(|e| e.to_string())?;
    o.definir_gasto_do_mes(gasto);
    Ok(Arc::new(o))
}

/// As peças que rodam uma triagem: o motor da fila, o registro do que saiu e o provedor.
pub struct Motor {
    /// A fila do motor (cache, ritmo, degraus).
    pub fila: Arc<Fila>,
    /// O registro de `saida_ia`, marcado com o id de cada rodada.
    pub registro: Arc<RegistroDaIa>,
    /// O provedor principal (serve ao teste de conexão e dá o nome da rodada).
    pub principal: Arc<dyn Provedor>,
    /// O rastro de falhas transitórias (o item falho é adiado, não "não classificado").
    pub rastro: RastroDeFalhas,
}

/// Monta o [`Motor`] sobre `banco` (o `inventario.sqlite`, já migrado). `agente = true` usa o
/// prompt `triagem-v2` (o agente contínuo); `false`, o `triagem-v1` da rodada manual. Os dois
/// convivem no cache porque a versão entra na chave.
///
/// # Errors
/// Falha ao abrir o banco ou o portão de rede do OpenRouter.
pub fn montar_motor(banco: &Path, insumos: &Insumos, agente: bool) -> Result<Motor, String> {
    let config = configuracao_salva(banco);
    let sem_chave = || -> Arc<dyn Provedor> {
        Arc::new(SemProvedor {
            motivo: "não há chave do OpenRouter no chaveiro".to_owned(),
            nome: "openrouter",
        })
    };
    let principal: Arc<dyn Provedor> = match config.principal {
        ProvedorEscolhido::ClaudeCli => claude(insumos),
        ProvedorEscolhido::OpenRouter => match &insumos.chave_do_openrouter {
            Some(f) => openrouter(
                banco,
                f,
                config.teto_da_rodada_centavos_usd,
                config.teto_do_mes_centavos_usd,
            )?,
            None => sem_chave(),
        },
    };
    // A reserva só entra ligada em Opções → IA, diferente do principal e com o que ela
    // precisa (a chave, no caso do OpenRouter).
    let reserva: Option<Arc<dyn Provedor>> = match config.reserva {
        Some(r) if r != config.principal => match r {
            ProvedorEscolhido::OpenRouter => insumos
                .chave_do_openrouter
                .as_ref()
                .map(|f| {
                    openrouter(
                        banco,
                        f,
                        config.teto_da_rodada_centavos_usd,
                        config.teto_do_mes_centavos_usd,
                    )
                })
                .transpose()?,
            ProvedorEscolhido::ClaudeCli => insumos.claude.as_ref().map(|_| claude(insumos)),
        },
        _ => None,
    };
    let registro = Arc::new(RegistroDaIa::abrir(banco, None).map_err(|e| e.to_string())?);
    let rastro = RastroDeFalhas::default();
    let usar_reserva = config.reserva.is_some_and(|r| r != config.principal);
    let fila = Arc::new(Fila::nova(
        rastro.embrulhar(Arc::clone(&principal)),
        reserva.map(|r| rastro.embrulhar(r)),
        Arc::clone(&registro) as Arc<dyn Registro>,
        if agente {
            configuracao_da_fila_v2(usar_reserva)
        } else {
            configuracao_da_fila(usar_reserva)
        },
    ));
    Ok(Motor {
        fila,
        registro,
        principal,
        rastro,
    })
}

/// Monta o serviço de triagem sobre `banco` (o `inventario.sqlite`, já migrado).
///
/// # Errors
/// Falha ao abrir o banco ou o portão de rede do OpenRouter.
pub fn montar(banco: &Path, insumos: &Insumos) -> Result<TriagemDoInventario, String> {
    let m = montar_motor(banco, insumos, false)?;
    Ok(TriagemDoInventario::nova(banco, m.fila, m.registro, m.principal)?.com_rastro(m.rastro))
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_ia::resolver_claude_em;
    use std::os::unix::fs::PermissionsExt;

    fn executavel(pasta: &Path, nome: &str) -> PathBuf {
        let c = pasta.join(nome);
        std::fs::write(&c, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o755)).unwrap();
        c
    }

    fn pasta(modo: u32) -> tempfile::TempDir {
        let p = tempfile::tempdir().unwrap();
        std::fs::set_permissions(p.path(), std::fs::Permissions::from_mode(modo)).unwrap();
        p
    }

    #[test]
    fn acha_o_claude_em_local_bin_mesmo_sendo_um_link() {
        let casa = pasta(0o755);
        let versoes = casa.path().join(".local/share/claude/versions");
        std::fs::create_dir_all(&versoes).unwrap();
        let real = executavel(&versoes, "2.1.0");
        let bin = casa.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(&real, bin.join("claude")).unwrap();
        for d in [
            casa.path().join(".local"),
            casa.path().join(".local/share"),
            casa.path().join(".local/share/claude"),
            versoes,
            bin.clone(),
        ] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let achado = resolver_claude_em(Some(casa.path()), None).unwrap();
        assert_eq!(achado, bin.join("claude"));
    }

    #[test]
    fn recusa_o_claude_em_pasta_que_outros_escrevem() {
        let aberta = pasta(0o777);
        executavel(aberta.path(), "claude");
        let path = std::env::join_paths([aberta.path()]).unwrap();
        // Nem o PATH nem o HOME do usuário valem se a pasta é gravável por qualquer um.
        assert_eq!(resolver_claude_em(None, Some(&path)), None);
    }

    #[test]
    fn ignora_pasta_relativa_do_path() {
        let path = std::env::join_paths(["bin", "."]).unwrap();
        assert_eq!(resolver_claude_em(None, Some(&path)), None);
    }

    #[test]
    fn path_do_usuario_serve_quando_a_pasta_e_de_confianca() {
        let boa = pasta(0o755);
        let c = executavel(boa.path(), "claude");
        let path = std::env::join_paths([boa.path()]).unwrap();
        assert_eq!(resolver_claude_em(None, Some(&path)), Some(c));
    }

    /// Um inventário vazio, migrado como o app o deixa (versão 3, com `meta`, `saida_ia`…).
    fn banco(dir: &Path) -> PathBuf {
        use sbf_inventario::banco::Inventario;
        use sbf_inventario::esquema::MIGRACAO_1_BASE_BUN;
        let caminho = dir.join("inventario.sqlite");
        {
            let conn = rusqlite::Connection::open(&caminho).unwrap();
            conn.execute_batch(MIGRACAO_1_BASE_BUN).unwrap();
            conn.execute_batch(
                "ALTER TABLE registro ADD COLUMN tipo_original TEXT;
                 ALTER TABLE registro ADD COLUMN contraparte TEXT;
                 ALTER TABLE registro ADD COLUMN contraparte_doc TEXT;
                 ALTER TABLE registro ADD COLUMN data_compra TEXT;
                 ALTER TABLE registro ADD COLUMN merchant_cnpj TEXT;
                 ALTER TABLE registro ADD COLUMN fatura_ref TEXT;
                 INSERT OR REPLACE INTO meta (chave, valor) VALUES ('schema_versao', '2');",
            )
            .unwrap();
        }
        let mut inv = Inventario::abrir(&caminho).unwrap();
        inv.migrar(dir.join("copia.sqlite")).unwrap();
        caminho
    }

    /// Um `claude` de mentira: lê a entrada e responde o JSON do modo headless.
    fn claude_falso(pasta: &Path) -> PathBuf {
        let c = pasta.join("claude");
        let saida = r#"{"type":"result","subtype":"success","is_error":false,"result":"{\"ok\":true}","total_cost_usd":0.001,"usage":{"input_tokens":10,"output_tokens":2},"modelUsage":{"claude-falso":{}}}"#;
        std::fs::write(
            &c,
            format!("#!/bin/sh\ncat >/dev/null\nprintf '%s' '{saida}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&c, std::fs::Permissions::from_mode(0o755)).unwrap();
        c
    }

    #[test]
    fn monta_sobre_o_inventario_e_testa_a_conexao_com_o_binario_falso() {
        use sbf_triagem::servico::{Conclusao, ServicoDeTriagem};
        let dir = pasta(0o755);
        let banco = banco(dir.path());
        let bin = pasta(0o755);
        let trabalho = pasta(0o700);
        let insumos = Insumos {
            claude: Some(claude_falso(bin.path())),
            chave_do_openrouter: None,
            pasta_de_trabalho: Some(trabalho.path().to_path_buf()),
        };
        let mut s = montar(&banco, &insumos).unwrap();
        // Montar não chamou nada; o teste só roda quando se pede, e fora do quadro.
        s.iniciar_teste_de_conexao();
        let mut chegou = Vec::new();
        for _ in 0..400 {
            chegou.extend(s.sondar());
            if !chegou.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        match chegou.as_slice() {
            [Conclusao::Teste(Ok(t))] => assert!(t.contains("claude-cli"), "{t}"),
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn sem_claude_o_teste_diz_que_esta_indisponivel_e_nao_roda_nada() {
        use sbf_triagem::servico::ServicoDeTriagem;
        let dir = pasta(0o755);
        let banco = banco(dir.path());
        let mut s = montar(&banco, &Insumos::default()).unwrap();
        let e = s.testar_conexao().unwrap_err();
        assert!(e.contains("não foi encontrado"), "{e}");
    }

    #[test]
    fn reserva_openrouter_so_entra_com_chave_e_ligada() {
        use sbf_triagem::servico::{ConfiguracaoDeIa, ServicoDeTriagem};
        let dir = pasta(0o755);
        let banco = banco(dir.path());
        let bin = pasta(0o755);
        let chave: Arc<dyn FonteDaChave> = Arc::new(|| None);
        let com_chave = Insumos {
            claude: Some(claude_falso(bin.path())),
            chave_do_openrouter: Some(chave),
            pasta_de_trabalho: None,
        };
        // Reserva desligada (o padrão): montar não cria o OpenRouter nem toca a rede.
        assert!(montar(&banco, &com_chave).is_ok());
        // Ligada em Opções → IA e com chave: monta, restaurando o gasto do mês do banco.
        let mut s = montar(&banco, &com_chave).unwrap();
        s.salvar_configuracao(ConfiguracaoDeIa {
            reserva: Some(ProvedorEscolhido::OpenRouter),
            ..ConfiguracaoDeIa::default()
        })
        .unwrap();
        assert!(montar(&banco, &com_chave).is_ok());
        // Ligada mas sem chave: monta só com o principal.
        assert!(
            montar(
                &banco,
                &Insumos {
                    chave_do_openrouter: None,
                    ..com_chave.clone()
                }
            )
            .is_ok()
        );
    }
}
