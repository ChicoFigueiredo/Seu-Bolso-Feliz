//! O ciclo de coleta: para cada conversa **incluída**, lê o que é novo desde o checkpoint,
//! baixa os anexos para uma pasta temporária privada, grava no inventário e avança o
//! checkpoint. Cancelável entre mensagens; uma falha numa mensagem para só aquela conversa e
//! deixa o checkpoint antes dela (a próxima rodada tenta de novo).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use crate::Resultado;
use crate::config::{Checkpoints, Config};
use crate::fonte::FonteDeMensagens;
use crate::gravador::{Contadores, Gravador};
use crate::mapeamento::{Item, itens_da};
use crate::modelo::Checkpoint;

/// Onde a coleta trabalha.
#[derive(Debug, Clone)]
pub struct Pastas {
    /// A pasta do cofre (acervo, `tmp/`).
    pub cofre: PathBuf,
}

/// O que um ciclo fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Conversas lidas.
    pub conversas: usize,
    /// Mensagens vistas.
    pub mensagens: usize,
    /// Os contadores do inventário.
    pub contadores: Contadores,
    /// Parou por cancelamento.
    pub interrompido: bool,
}

fn pasta_temporaria(cofre: &Path) -> Resultado<PathBuf> {
    let p = cofre.join("tmp").join("mensageiros");
    std::fs::create_dir_all(&p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700));
    }
    Ok(p)
}

/// Roda um ciclo de `fonte`. Só toca em conversa que a configuração inclui.
///
/// # Errors
/// Falha do banco ou do disco, ou da fonte ao listar conversas. Falha por mensagem não é
/// erro do ciclo: vira contagem em `erros`.
pub fn coletar(
    fonte: &mut dyn FonteDeMensagens,
    conexao: &Connection,
    pastas: &Pastas,
    config: &Config,
    checkpoints: &mut Checkpoints,
    cancelar: &AtomicBool,
) -> Resultado<Relatorio> {
    let plataforma = fonte.plataforma();
    let escolha = config.escolha(plataforma);
    let mut relatorio = Relatorio::default();
    // Nada incluído: nem pergunta à fonte (não abre sessão, não lê nada).
    if !escolha.ligada || escolha.incluidas.is_empty() {
        return Ok(relatorio);
    }
    let tmp = pasta_temporaria(&pastas.cofre)?;
    let conversas = fonte.listar_conversas()?;
    let mut gravador = Gravador::novo(conexao, &pastas.cofre, plataforma);
    'conversas: for conversa in conversas.iter().filter(|c| escolha.coleta(&c.id)) {
        if cancelar.load(Ordering::Relaxed) {
            relatorio.interrompido = true;
            break;
        }
        relatorio.conversas += 1;
        let desde = checkpoints.de(plataforma, &conversa.id);
        let mensagens = fonte.mensagens_novas(&conversa.id, &desde, config.lote.max(1))?;
        let mut ponto: Option<Checkpoint> = None;
        for m in &mensagens {
            if cancelar.load(Ordering::Relaxed) {
                relatorio.interrompido = true;
                if let Some(p) = ponto.take() {
                    checkpoints.avancar(plataforma, &conversa.id, p);
                }
                break 'conversas;
            }
            relatorio.mensagens += 1;
            let mut ok = true;
            for item in itens_da(plataforma, m) {
                let r = match &item {
                    Item::Anexo {
                        origem,
                        nome,
                        extensao,
                    } => baixar_e_gravar(fonte, &mut gravador, m, origem, nome, extensao, &tmp),
                    Item::Texto { origem, conteudo } => {
                        texto_e_gravar(&mut gravador, m, origem, conteudo, &tmp)
                    }
                };
                if r.is_err() {
                    gravador.contadores.erros += 1;
                    ok = false;
                }
            }
            if !ok {
                break; // o checkpoint fica antes desta mensagem
            }
            ponto = Some(Checkpoint::de(m));
        }
        if let Some(p) = ponto {
            checkpoints.avancar(plataforma, &conversa.id, p);
        }
    }
    relatorio.contadores = gravador.fechar(if relatorio.interrompido {
        "interrompida"
    } else {
        "concluida"
    })?;
    Ok(relatorio)
}

fn baixar_e_gravar(
    fonte: &mut dyn FonteDeMensagens,
    gravador: &mut Gravador<'_>,
    m: &crate::modelo::Mensagem,
    origem: &str,
    nome: &str,
    extensao: &str,
    tmp: &Path,
) -> Resultado<()> {
    // Já gravada: nem baixa a mídia de novo.
    if gravador_ja_tem(gravador, origem)? {
        gravador.contadores.vistos += 1;
        gravador.contadores.repetidos += 1;
        return Ok(());
    }
    let arquivo = tmp.join(format!("baixando-{}.{extensao}", std::process::id()));
    let r = fonte.baixar_anexo(m, &arquivo).and_then(|()| {
        gravador
            .gravar(m, origem, nome, extensao, &arquivo)
            .map(|_| ())
    });
    let _ = std::fs::remove_file(&arquivo);
    r
}

fn texto_e_gravar(
    gravador: &mut Gravador<'_>,
    m: &crate::modelo::Mensagem,
    origem: &str,
    conteudo: &str,
    tmp: &Path,
) -> Resultado<()> {
    let arquivo = tmp.join(format!("texto-{}.txt", std::process::id()));
    std::fs::write(&arquivo, conteudo.as_bytes())?;
    let r = gravador
        .gravar(m, origem, "mensagem.txt", "txt", &arquivo)
        .map(|_| ());
    let _ = std::fs::remove_file(&arquivo);
    r
}

fn gravador_ja_tem(gravador: &Gravador<'_>, origem: &str) -> Resultado<bool> {
    gravador.ja_tem(origem)
}
