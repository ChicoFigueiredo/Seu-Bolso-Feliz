//! `sbf worker`: o modo sem janela (ADR-012, ADR-014). Roda como serviço do usuário e
//! mantém o inventário em dia com a janela fechada.
//!
//! ```text
//! sbf worker rodar       --inventario <pasta-do-cofre>   o laço (o que a unit executa)
//! sbf worker instalar    --inventario <pasta-do-cofre> [--sem-ligar]
//! sbf worker desinstalar [--sem-desligar]
//! sbf worker estado
//! ```
//!
//! `instalar` escreve a unit systemd do usuário e a liga (`enable --now`); com `--sem-ligar`
//! só escreve o arquivo. No Windows e no macOS ainda não há serviço: os comandos de serviço
//! respondem "ainda não suportado" (`rodar` à mão funciona).
//!
//! O que o worker usa da IA: só o `claude` do usuário, achado por caminho de confiança. O
//! OpenRouter exige a chave do chaveiro aberto, e o worker não pede senha: sem janela não há
//! quem digite.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sbf_coleta::nuvem::{ColetaDeNuvem, Nuvem, PastasDoCofre};
use sbf_coleta::pastas::{Opcoes, Varredor};
use sbf_triagem::agente::{Agente, rodar_ciclo_com};
use sbf_triagem::rodada::Progresso;
use sbf_worker::estado::{EstadoDoWorker, agora, texto_da_barra};
use sbf_worker::laco::{
    ArquivoNovo, ColetorDeMensageiros, ColetorDeNuvem, Dados, ResumoDaNuvem, ResumoDaTriagem,
    ResumoDeMensageiros, Triador, Worker, carregar_config,
};
use sbf_worker::registro::Falha;
use sbf_worker::trava::Trava;
use sbf_worker::unidade::{self, SystemctlDoSistema};

use crate::triagem::{Insumos, Motor, montar_motor};

const USO: &str = "uso: sbf worker rodar --inventario <pasta-do-cofre> | sbf worker instalar --inventario <pasta-do-cofre> [--sem-ligar] | sbf worker desinstalar [--sem-desligar] | sbf worker estado";

fn valor<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// A pasta de dados do app, fora do cofre (onde a janela guarda `nuvens.json`).
fn pasta_de_dados(cofre: &Path) -> Result<PathBuf, String> {
    let pasta = sbf_chaveiro::pasta_de_dados_do_app().map_err(|e| e.to_string())?;
    sbf_chaveiro::garantir_fora_do_cofre(&pasta, cofre).map_err(|e| e.to_string())?;
    Ok(pasta)
}

/// Lê o estado do worker para a barra de status da janela.
#[must_use]
pub fn texto_da_barra_do_worker() -> String {
    let estado = sbf_chaveiro::pasta_de_dados_do_app()
        .ok()
        .and_then(|p| EstadoDoWorker::ler(&p.join("worker-estado.json")));
    texto_da_barra(estado.as_ref(), agora())
}

/// A coleta de nuvem de verdade: lista, baixa os candidatos para `entrada/<pasta>`, indexa o
/// que baixou e liga as origens ao conteúdo.
struct NuvemReal {
    dados: Dados,
}

impl ColetorDeNuvem for NuvemReal {
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDaNuvem, Falha> {
        let rclone = sbf_janela::nuvens::rclone_do_sbf(&self.dados.cofre, None);
        let conn = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        let mut total = ResumoDaNuvem::default();
        for nuvem in [Nuvem::onedrive(), Nuvem::gdrive()] {
            if cancelar.load(Ordering::Relaxed) {
                break;
            }
            let fonte = nuvem.fonte;
            let coleta = match ColetaDeNuvem::detectando(
                &rclone,
                nuvem,
                PastasDoCofre::de(&self.dados.cofre),
            ) {
                Ok(c) => c,
                // Essa nuvem não está conectada: segue para a outra.
                Err(sbf_coleta::Erro::SemRemoto(_)) => continue,
                Err(_) => return Err(Falha::Externo),
            };
            let r = coleta
                .coletar(&conn, cancelar, &|_| {})
                .map_err(|e| match e {
                    sbf_coleta::Erro::Banco(_) => Falha::Banco,
                    sbf_coleta::Erro::Io(_) => Falha::Disco,
                    _ => Falha::Externo,
                })?;
            total.listados += r.listados;
            total.baixados += r.baixados;
            if r.baixados > 0 && !r.interrompida {
                // A indexação do que baixou é a etapa E2 (todo o material de `entrada/` é A).
                let mut v = Varredor::novo(
                    &conn,
                    fonte,
                    &r.destino,
                    Opcoes {
                        cofre: self.dados.cofre.clone(),
                        entrada: true,
                        ..Opcoes::default()
                    },
                )
                .map_err(|_| Falha::Banco)?;
                let completa = v.varrer(cancelar).map_err(|_| Falha::Disco)?;
                v.finalizar(if completa {
                    "concluida"
                } else {
                    "interrompida"
                })
                .map_err(|_| Falha::Banco)?;
                coleta.ligar_hashes(&conn).map_err(|_| Falha::Banco)?;
            }
        }
        Ok(total)
    }
}

/// Os mensageiros de verdade: Telegram pelo export e WhatsApp pela ponte (ADR-017), só as
/// conversas que o usuário incluiu em Opções › Mensageiros.
struct MensageirosReais {
    dados: Dados,
}

impl ColetorDeMensageiros for MensageirosReais {
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDeMensageiros, Falha> {
        let conn = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        // O worker não pede senha: se o chaveiro não abre sozinho, o WhatsApp falha e o
        // Telegram segue.
        let chaveiro = || {
            sbf_chaveiro::Chaveiro::abrir_na_pasta_padrao()
                .map_err(|e| sbf_mensageiros::Erro::Chaveiro(e.to_string()))
        };
        let r = sbf_mensageiros::ciclo::rodar(
            &conn,
            &self.dados.pasta,
            &self.dados.cofre,
            &chaveiro,
            cancelar,
        )
        .map_err(|e| match e {
            sbf_mensageiros::Erro::Banco(_) => Falha::Banco,
            sbf_mensageiros::Erro::Io(_) => Falha::Disco,
            _ => Falha::Externo,
        })?;
        if r.falhas > 0 && r.mensagens == 0 {
            return Err(Falha::Externo);
        }
        Ok(ResumoDeMensageiros {
            mensagens: r.mensagens,
            novos: r.novos,
        })
    }
}

/// O agente de triagem de verdade: o motor sobre o `claude` do usuário e o estado do agente.
struct TriadorReal {
    dados: Dados,
    insumos: Insumos,
    motor: Option<Motor>,
    agente: Option<Agente>,
    /// A busca por texto parecido (spec 10), quando o modelo e o índice existem.
    busca: Option<sbf_indice::BuscaLocal>,
}

impl Triador for TriadorReal {
    fn ciclo(&mut self, lote: usize, cancelar: &AtomicBool) -> Result<ResumoDaTriagem, Falha> {
        if self.motor.is_none() {
            self.motor = Some(
                montar_motor(&self.dados.banco, &self.insumos, true)
                    .map_err(|_| Falha::Provedor)?,
            );
        }
        if self.agente.is_none() {
            self.agente = Some(Agente::abrir(self.dados.agente()).map_err(|_| Falha::Banco)?);
        }
        let (Some(motor), Some(agente)) = (self.motor.as_ref(), self.agente.as_ref()) else {
            return Err(Falha::Outra);
        };
        let conn = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        let controle = sbf_ia::Controle::novo();
        if cancelar.load(Ordering::Relaxed) {
            controle.cancelar();
        }
        let progresso = Mutex::new(Progresso::ocioso());
        if self.busca.is_none() {
            self.busca = crate::rag::busca_local(&self.dados.pasta);
        }
        let r = rodar_ciclo_com(
            &conn,
            agente,
            &motor.fila,
            motor.principal.nome(),
            lote,
            &controle,
            &progresso,
            &|id| motor.registro.definir_rodada(Some(id)),
            Some(motor.rastro.clone()),
            self.busca.as_ref().map(|b| b as &dyn sbf_indice::Busca),
        )
        .map_err(|_| Falha::Banco)?;
        motor.registro.definir_rodada(None);
        if r.aviso.is_some() {
            return Err(Falha::Provedor);
        }
        Ok(ResumoDaTriagem {
            enviados: r.enviados,
            sugestoes: r.sugestoes,
            por_regra: r.por_regra,
            na_fila: r.na_fila,
        })
    }
}

fn rodar_o_laco(cofre: &Path) -> ExitCode {
    let pasta = match pasta_de_dados(cofre) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sbf: {e}");
            return ExitCode::from(3);
        }
    };
    let dados = Dados::novos(cofre, &pasta);
    if let Err(e) = std::fs::create_dir_all(&dados.pasta) {
        eprintln!("sbf: não deu para criar a pasta de dados: {e}");
        return ExitCode::from(3);
    }
    let _trava = match Trava::tomar(&dados.lock()) {
        Ok(t) => t,
        Err(sbf_worker::Erro::JaRodando) => {
            eprintln!("sbf: já há um worker rodando para esta pasta de dados.");
            return ExitCode::from(4);
        }
        Err(e) => {
            eprintln!("sbf: {e}");
            return ExitCode::from(3);
        }
    };
    let config = match carregar_config(&dados) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sbf: {e}");
            return ExitCode::from(3);
        }
    };
    if !dados.banco.exists() {
        eprintln!("sbf: não achei o inventário na pasta do cofre.");
        return ExitCode::from(3);
    }
    let parar = Arc::new(AtomicBool::new(false));
    {
        let p = Arc::clone(&parar);
        // SIGINT e SIGTERM (e Ctrl+C no Windows): parada limpa, o laço termina o passo.
        if ctrlc::set_handler(move || p.store(true, Ordering::SeqCst)).is_err() {
            eprintln!("sbf: não deu para instalar o tratamento de sinal.");
            return ExitCode::from(3);
        }
    }
    let nuvem: Box<dyn ColetorDeNuvem> = Box::new(NuvemReal {
        dados: dados.clone(),
    });
    let triador: Box<dyn Triador> = Box::new(TriadorReal {
        dados: dados.clone(),
        insumos: Insumos::do_ambiente(None),
        motor: None,
        agente: None,
        busca: None,
    });
    let mensageiros = Box::new(MensageirosReais {
        dados: dados.clone(),
    });
    let telegram = Box::new(crate::telegram::TelegramReal {
        dados: dados.clone(),
    });
    let dados_das_pastas = dados.clone();
    let dados_dos_roteiros = dados.clone();
    let dados_do_gmail = dados.clone();
    let dados_do_rag = dados.clone();
    let dados_do_ocr = dados.clone();
    let dados_dos_padroes = dados.clone();
    match Worker::novo(dados, config, Some(nuvem), Some(triador)) {
        Ok(w) => {
            let mut w = w
                .com_mensageiros(mensageiros)
                .com_telegram(telegram)
                .com_roteiros(Box::new(crate::roteiros::RoteirosReais::novo(
                    dados_dos_roteiros,
                )))
                .com_indice(Box::new(crate::rag::IndexadorReal::novo(
                    dados_do_rag.clone(),
                )))
                .com_pontos(Box::new(crate::rag::AnalistaReal::novo(dados_do_rag)))
                .com_ocr(Box::new(crate::ocr::LeitorReal::novo(dados_do_ocr)))
                .com_passada_de_padroes(Box::new(crate::padroes::PassadaReal::novo(
                    dados_dos_padroes,
                    crate::script::executor_de_script().ok(),
                )))
                .com_reconhecimento_das_pastas(Box::new(ReconhecimentoDasPastas::novo(
                    dados_das_pastas,
                )));
            // O e-mail sempre liga: a cada intervalo o coletor olha as contas conectadas (chaveiro,
            // ambiente ou .env) na hora, então um login feito com o worker rodando vale sem
            // reiniciar. Sem conta conectada não é falha. Autorização vencida suspende (veja
            // `ColetorDeGmail::pode_tentar_de_novo`).
            w = w.com_gmail(Box::new(crate::gmail::GmailReal {
                dados: dados_do_gmail,
            }));
            w.rodar(&parar);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(3)
        }
    }
}

fn sobre_o_servico(sub: &str, args: &[String]) -> ExitCode {
    let Some(config_home) = unidade::config_home() else {
        eprintln!("sbf: não achei a pasta de configuração do usuário (HOME).");
        return ExitCode::from(3);
    };
    let sistema = SystemctlDoSistema;
    let r = match sub {
        "instalar" => {
            let Some(cofre) = valor(args, "--inventario") else {
                eprintln!("sbf: falta --inventario <pasta-do-cofre>.\n{USO}");
                return ExitCode::from(2);
            };
            let exe = match std::env::current_exe() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("sbf: {e}");
                    return ExitCode::from(3);
                }
            };
            let cofre = std::fs::canonicalize(cofre).unwrap_or_else(|_| PathBuf::from(cofre));
            unidade::instalar(
                &exe,
                &cofre,
                &config_home,
                !args.iter().any(|a| a == "--sem-ligar"),
                &sistema,
            )
            .map(|arq| {
                println!("sbf: unit escrita em {}", arq.display());
            })
        }
        "desinstalar" => unidade::desinstalar(
            &config_home,
            !args.iter().any(|a| a == "--sem-desligar"),
            &sistema,
        )
        .map(|existia| {
            println!(
                "{}",
                if existia {
                    "sbf: serviço removido."
                } else {
                    "sbf: o serviço não estava instalado."
                }
            );
        }),
        _ => unidade::estado(&config_home, &sistema).map(|e| {
            println!(
                "serviço: {} · habilitado: {} · ativo: {}",
                if e.instalada {
                    "instalado"
                } else {
                    "não instalado"
                },
                e.habilitada,
                e.ativa
            );
            println!("{}", texto_da_barra_do_worker());
        }),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e @ sbf_worker::Erro::NaoSuportado) => {
            eprintln!("sbf: {e}");
            ExitCode::from(5)
        }
        Err(e) => {
            eprintln!("sbf: {e}");
            ExitCode::from(1)
        }
    }
}

/// `sbf worker ...`.
pub fn rodar(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("rodar") => match valor(args, "--inventario") {
            Some(cofre) => rodar_o_laco(Path::new(cofre)),
            None => {
                eprintln!("sbf: falta --inventario <pasta-do-cofre>.\n{USO}");
                ExitCode::from(2)
            }
        },
        Some(sub @ ("instalar" | "desinstalar" | "estado")) => sobre_o_servico(sub, args),
        _ => {
            eprintln!("{USO}");
            ExitCode::from(2)
        }
    }
}

/// O reconhecimento dos arquivos novos das pastas vigiadas: o `ReconhecedorDoCofre` com as
/// regras, sem IA, com o canal da pasta (Caixa de entrada). Monta uma vez, na primeira
/// chegada, porque abrir o chaveiro e as regras custa e a maioria dos passos não tem arquivo.
struct ReconhecimentoDasPastas {
    dados: Dados,
    reconhecedor: Option<sbf_reconhecimento::ReconhecedorDoCofre>,
}

impl ReconhecimentoDasPastas {
    fn novo(dados: Dados) -> Self {
        Self {
            dados,
            reconhecedor: None,
        }
    }
}

impl sbf_worker::laco::ReconhecedorDePastas for ReconhecimentoDasPastas {
    fn reconhecer(&mut self, a: &ArquivoNovo, cancelar: &AtomicBool) -> Result<(), Falha> {
        use sbf_inventario::caixa_de_entrada::{Canal, Chegada};
        if self.reconhecedor.is_none() {
            let chaveiro = sbf_chaveiro::Chaveiro::abrir_na_pasta_padrao().ok();
            let r = crate::reconhecer::montar(&self.dados.cofre, chaveiro, false)
                .map_err(|_| Falha::Externo)?;
            self.reconhecedor = Some(r);
        }
        let Some(r) = self.reconhecedor.as_ref() else {
            return Err(Falha::Outra);
        };
        // A pasta do celular tem a fonte `celular` no `worker.json`; as outras são pastas.
        let chegada = Chegada {
            canal: Canal::da_fonte_da_pasta(&a.fonte),
            reabrir: false,
        };
        let nome = a
            .caminho
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // O original sumiu desde que foi indexado: a cópia do acervo ainda serve; sem ela, a
        // Caixa de entrada guarda a falha (Precisa revisar), e o achado não é tentado de novo.
        let caminho = if a.caminho.is_file() {
            a.caminho.clone()
        } else if let Ok(c) = r.arquivo_do_conteudo(&a.sha256) {
            c
        } else {
            r.registrar_falha_da_leitura(chegada, &a.sha256, &nome);
            return Err(Falha::Disco);
        };
        r.reconhecer_chegada(chegada, &caminho, &mut |_| {}, cancelar)
            .map(|_| ())
            .map_err(|e| match e {
                sbf_reconhecimento::ErroDoReconhecimento::Banco(_) => Falha::Banco,
                sbf_reconhecimento::ErroDoReconhecimento::Arquivo(_) => Falha::Disco,
                sbf_reconhecimento::ErroDoReconhecimento::Cancelado => Falha::Outra,
            })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valor_da_flag() {
        let a: Vec<String> = ["rodar", "--inventario", "/c"].map(String::from).to_vec();
        assert_eq!(valor(&a, "--inventario"), Some("/c"));
        assert_eq!(valor(&a, "--x"), None);
    }

    #[test]
    fn sem_cofre_ou_subcomando_desconhecido_e_uso_2() {
        assert_eq!(rodar(&["rodar".to_owned()]), ExitCode::from(2),);
        assert_eq!(rodar(&["voar".to_owned()]), ExitCode::from(2));
        assert_eq!(rodar(&[]), ExitCode::from(2));
    }
}
