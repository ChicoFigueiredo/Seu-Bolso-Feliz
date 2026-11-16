//! O modo `sbf --inventario <pasta-do-cofre>`: abre o `inventario.sqlite` de verdade.
//!
//! - Abrir não grava nada (INV-01). Se o banco precisa migrar, a janela mostra o plano e a
//!   cópia que vai ser feita, e só migra com o seu OK. A migração roda numa thread, com
//!   andamento: num inventário real ela leva dezenas de segundos, e a janela não pode ficar
//!   "não respondendo".
//! - Com o banco em dia, abre a janela completa: `LeitorSqlite` como `Consulta` (conexão
//!   só de leitura), o próprio `Inventario` como `Comandos`, e as Ferramentas (PDF
//!   protegido, estante, cópia de segurança, chaveiro) ligadas ao banco e ao cofre.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, Ui};
use sbf_disco::Cifra;
use sbf_inventario::banco::{EtapaDaMigracao, Inventario, MigracaoFeita, PlanoDeMigracao};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_janela::ferramentas::{self, Ferramentas};
use sbf_janela::{EstadoDaBarra, Janela, componentes, tema};

type Migracao = JoinHandle<(Inventario, Result<MigracaoFeita, String>)>;

/// O que a tela do inventário real mostra.
enum Tela {
    /// O plano de migração, esperando o OK.
    Migrar {
        plano: PlanoDeMigracao,
        copia: PathBuf,
        erro: Option<String>,
    },
    /// Migrando numa thread.
    Migrando {
        inicio: Instant,
        copia: PathBuf,
        tarefa: Option<Migracao>,
        etapas: std::sync::mpsc::Receiver<String>,
        etapa: String,
    },
    /// Não deu para abrir a janela completa.
    Pronto { aviso: String },
}

pub struct AppDoInventario {
    inventario: Option<Inventario>,
    pasta: PathBuf,
    tela: Tela,
    janela: Option<crate::App>,
}

/// Abre o banco da pasta do cofre sem gravar nada.
pub fn abrir(pasta: &Path) -> Result<AppDoInventario, String> {
    let banco = pasta.join("inventario.sqlite");
    if !banco.is_file() {
        return Err(format!("não achei {} (nada foi criado)", banco.display()));
    }
    let inventario = Inventario::abrir(&banco).map_err(|e| e.to_string())?;
    let plano = inventario.plano().map_err(|e| e.to_string())?;
    let precisa = plano.precisa_migrar();
    let tela = Tela::Migrar {
        copia: inventario.caminho_sugerido_da_copia(),
        plano,
        erro: None,
    };
    let mut app = AppDoInventario {
        inventario: Some(inventario),
        pasta: pasta.to_path_buf(),
        tela,
        janela: None,
    };
    if !precisa {
        app.ligar(None);
    }
    Ok(app)
}

impl AppDoInventario {
    /// Abre a leitura e troca esta tela pela janela completa.
    fn ligar(&mut self, aviso: Option<String>) {
        let Some(inventario) = self.inventario.take() else {
            return;
        };
        match LeitorSqlite::abrir(inventario.caminho()) {
            Ok(leitor) => {
                let cifra = sbf_disco::cifra_do_volume(&self.pasta);
                let barra = EstadoDaBarra {
                    inventario: format!(
                        "{} · inventario.sqlite",
                        self.pasta.file_name().map_or_else(
                            || self.pasta.display().to_string(),
                            |n| n.to_string_lossy().into_owned()
                        )
                    ),
                    disco_cifrado: Some(
                        match cifra {
                            Cifra::Cifrado => "sim",
                            Cifra::NaoCifrado => "não",
                            Cifra::Desconhecido => "não se sabe",
                        }
                        .to_owned(),
                    ),
                    aviso_de_disco: sbf_disco::texto_do_aviso(cifra).map(str::to_owned),
                    ..Default::default()
                };
                let mut janela = Janela::nova();
                janela.ligacoes.cofre = true;
                // O controle mensal (spec 11): o planejamento do cofre, quando já existe.
                let arquivo_do_plano = self.pasta.join("planejamento.sqlite");
                if arquivo_do_plano.exists() {
                    match sbf_planejamento::Banco::abrir(&arquivo_do_plano) {
                        Ok(banco) => janela.ligar_planejamento(Box::new(
                            sbf_planejamento::PlanejamentoSqlite::novo(
                                banco,
                                crate::mcp::saldos_do_cofre(&self.pasta),
                            ),
                        )),
                        Err(e) => eprintln!("sbf: o planejamento não abriu ({e})"),
                    }
                }
                let mut f =
                    Ferramentas::nova(inventario.caminho().to_path_buf(), self.pasta.clone());
                // Só lê o estado: não pede senha. Se o keyring travar, a barra mostra "—".
                match ferramentas::abrir_chaveiro_com_espera(Duration::from_secs(3)) {
                    Ok(c) => f.chaveiro = Some(c),
                    Err(e) => f.erro_do_chaveiro = Some(e),
                }
                // A prévia lê a cópia do acervo; PDF protegido só com o chaveiro aberto.
                janela.previas.definir_pasta(self.pasta.clone());
                if let Some(c) = f.chaveiro.clone() {
                    janela
                        .previas
                        .definir_candidatas(sbf_janela::previa::gerador_do_inventario(
                            inventario.caminho().to_path_buf(),
                            c,
                        ));
                }
                f.carregar_preferencias();
                f.faxinar_sobras_em_segundo_plano();
                let chaveiro_da_ia = f.chaveiro.clone();
                let mut janela_chaveiro_para_perguntar = f.chaveiro.clone();
                let chaves_da_ia = f.chaveiro.clone();
                let chaveiro_dos_roteiros = f.chaveiro.clone();
                janela.ferramentas = Some(f);
                // Conexões › Roteiros (spec 10, C1): o Chrome, o chaveiro e o `claude` do
                // usuário. Nada roda até você clicar em Executar ou em Gravar novo.
                if let Ok(dados) = sbf_chaveiro::pasta_de_dados_do_app()
                    && sbf_chaveiro::garantir_fora_do_cofre(&dados, &self.pasta).is_ok()
                {
                    janela.roteiros.ligar(
                        crate::roteiros::ambiente(&self.pasta, &dados),
                        Some(std::sync::Arc::new(crate::roteiros::IaDoInventario {
                            banco: inventario.caminho().to_path_buf(),
                            insumos: crate::triagem::Insumos::do_ambiente(
                                chaveiro_dos_roteiros.clone(),
                            ),
                        })),
                        chaveiro_dos_roteiros,
                    );
                }
                // A triagem por IA (spec 09): o `claude` do usuário como principal e, só se
                // houver chave no chaveiro e a reserva ligada, o OpenRouter. Nada é chamado
                // aqui: o provedor só roda quando você clica em Iniciar ou em Testar conexão.
                match crate::triagem::montar(
                    inventario.caminho(),
                    &crate::triagem::Insumos::do_ambiente(chaveiro_da_ia),
                ) {
                    Ok(servico) => {
                        // O painel do agente contínuo lê o estado dele na pasta de dados.
                        let servico = match sbf_chaveiro::pasta_de_dados_do_app() {
                            Ok(p) => servico.com_agente(p.join("agente.sqlite")),
                            Err(_) => servico,
                        };
                        janela.ligar_triagem(Box::new(servico));
                    }
                    Err(e) if janela.mensagem.is_none() => {
                        janela.mensagem = Some(sbf_janela::janela::Mensagem {
                            texto: format!("Triagem por IA indisponível: {e}"),
                            erro: true,
                        });
                    }
                    Err(_) => {}
                }
                // Análise (spec 10): Perguntar (índice local + o motor de IA de Opções → IA) e
                // Pontos de atenção (análise por regras, sem IA). O modelo de busca só é
                // baixado pelo botão da própria área Perguntar, depois do aviso.
                if let Ok(pasta_de_dados) = sbf_chaveiro::pasta_de_dados_do_app() {
                    let dados = sbf_worker::laco::Dados::novos(&self.pasta, &pasta_de_dados);
                    janela.ligar_perguntar(Box::new(crate::rag::ServicoReal::novo(
                        dados.clone(),
                        crate::triagem::Insumos::do_ambiente(janela_chaveiro_para_perguntar.take()),
                    )));
                    if let Ok(armazem) =
                        sbf_pontos::Armazem::abrir(crate::rag::caminho_dos_pontos(&pasta_de_dados))
                    {
                        let (cofre, banco, pasta) = (
                            dados.cofre.clone(),
                            dados.banco.clone(),
                            dados.pasta.clone(),
                        );
                        janela.ligar_pontos(Box::new(sbf_pontos::PontosLocais::novo(
                            armazem,
                            move |_| crate::rag::analisar_pontos(&cofre, &banco, &pasta),
                        )));
                    }
                }
                // Opções → IA → Chaves e contas: o mesmo chaveiro e o mesmo `claude` do provedor.
                janela.ligar_chaves_de_ia(
                    chaves_da_ia
                        .map(|c| Box::new(c) as Box<dyn sbf_janela::chaves_ia::GuardaDeChaves>),
                    crate::triagem::resolver_claude(),
                );
                janela.pre_carregar_visao_geral(inventario.caminho().to_path_buf());
                if let Some(t) = aviso {
                    janela.mensagem = Some(sbf_janela::janela::Mensagem {
                        texto: t,
                        erro: false,
                    });
                }
                // O livro-caixa: a estação real (servidor de `SBF_SERVIDOR`), pareada ou não.
                let (consulta, ligacao) =
                    crate::estacao::ligar(&mut janela, Box::new(leitor), Some(&self.pasta));
                match crate::App::novo(janela, consulta, Box::new(inventario), barra) {
                    Ok(mut app) => {
                        app.estacao = ligacao;
                        app.mostrar_worker = true;
                        self.janela = Some(app);
                    }
                    Err(e) => {
                        self.tela = Tela::Pronto {
                            aviso: format!("Os temas não carregaram: {e}"),
                        }
                    }
                }
            }
            Err(e) => {
                self.tela = Tela::Pronto {
                    aviso: format!("O banco abriu, mas a leitura não: {e}"),
                };
                self.inventario = Some(inventario);
            }
        }
    }

    fn comecar_migracao(&mut self, copia: PathBuf) {
        let Some(mut inventario) = self.inventario.take() else {
            return;
        };
        let destino = copia.clone();
        let (tx, etapas) = std::sync::mpsc::channel();
        let tarefa = std::thread::spawn(move || {
            let andamento = |e: EtapaDaMigracao| {
                let _ = tx.send(texto_da_etapa(&e));
            };
            let r = inventario
                .migrar_com_andamento(&destino, &andamento)
                .map_err(|e| e.to_string());
            (inventario, r)
        });
        self.tela = Tela::Migrando {
            inicio: Instant::now(),
            copia,
            tarefa: Some(tarefa),
            etapas,
            etapa: "começando…".to_owned(),
        };
    }

    /// A migração terminou? Liga a janela (ou volta ao plano com o erro).
    fn sondar_migracao(&mut self) {
        let Tela::Migrando {
            tarefa,
            etapas,
            etapa,
            ..
        } = &mut self.tela
        else {
            return;
        };
        while let Ok(e) = etapas.try_recv() {
            *etapa = e;
        }
        if !tarefa.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        let Some(h) = tarefa.take() else { return };
        match h.join() {
            Ok((inventario, Ok(feita))) => {
                self.inventario = Some(inventario);
                let aviso = format!(
                    "Migrado da versão {} para a {}.{}",
                    feita.versao_inicial,
                    feita.versao_final,
                    feita
                        .copia
                        .map(|c| format!(" A cópia de antes está em {}.", c.display()))
                        .unwrap_or_default()
                );
                self.ligar(Some(aviso));
            }
            Ok((inventario, Err(e))) => {
                self.tela = match inventario.plano() {
                    Ok(plano) => Tela::Migrar {
                        plano,
                        copia: inventario.caminho_sugerido_da_copia(),
                        erro: Some(format!("A migração não foi feita: {e}")),
                    },
                    Err(e2) => Tela::Pronto {
                        aviso: format!(
                            "A migração falhou ({e}) e o banco não pôde ser relido ({e2})."
                        ),
                    },
                };
                self.inventario = Some(inventario);
            }
            Err(_) => {
                self.tela = Tela::Pronto {
                    aviso: "A migração parou no meio (a thread caiu). O banco e a cópia ficam como estão."
                        .to_owned(),
                };
            }
        }
    }
}

fn texto_da_etapa(e: &EtapaDaMigracao) -> String {
    match e {
        EtapaDaMigracao::Copia => "Copiando o banco (cópia de antes da migração)…".to_owned(),
        EtapaDaMigracao::ConferenciaDaCopia => "Conferindo a cópia…".to_owned(),
        EtapaDaMigracao::Adocao => "Adotando o banco do Bun…".to_owned(),
        EtapaDaMigracao::Migracao { versao, descricao } => {
            format!("Migração {versao}: {descricao}…")
        }
        EtapaDaMigracao::Conferencia => "Conferindo o banco migrado…".to_owned(),
    }
}

impl eframe::App for AppDoInventario {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.janela {
            app.ui(ui, frame);
            return;
        }
        self.sondar_migracao();
        if self.janela.is_some() {
            ui.ctx().request_repaint();
            return;
        }
        let p = tema::paleta(ui.ctx());
        let caminho = self.pasta.join("inventario.sqlite").display().to_string();
        let mut migrar = None;
        let mut sair = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.superficie_0)
                    .inner_margin(egui::Margin::same(40)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(640.0);
                    componentes::cartao(ui, |ui| {
                        ui.set_width(600.0);
                        ui.label(RichText::new(&caminho).size(12.0).color(p.tinta_suave));
                        ui.add_space(6.0);
                        match &self.tela {
                            Tela::Migrar { plano, copia, erro } => {
                                componentes::titulo(ui, "O inventário precisa ser migrado", 20.0);
                                ui.label(format!(
                                    "Da versão {} para a {}. Nada muda no banco até você confirmar.",
                                    plano.versao_atual, plano.versao_final
                                ));
                                ui.add_space(6.0);
                                for passo in &plano.passos {
                                    ui.label(format!(
                                        "• versão {}: {}",
                                        passo.versao, passo.descricao
                                    ));
                                }
                                ui.add_space(6.0);
                                if plano.faz_copia {
                                    componentes::aviso(
                                        ui,
                                        "backup",
                                        &format!(
                                            "Antes, uma cópia do banco vai para {}",
                                            copia.display()
                                        ),
                                        p.info_sutil,
                                        p.info_borda,
                                        p.info,
                                    );
                                }
                                if let Some(e) = erro {
                                    componentes::aviso(
                                        ui,
                                        "error",
                                        e,
                                        p.perigo_sutil,
                                        p.borda,
                                        p.perigo,
                                    );
                                }
                                ui.add_space(10.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button(RichText::new("Migrar").strong()).clicked() {
                                        migrar = Some(copia.clone());
                                    }
                                    if ui.button("Sair sem mudar nada").clicked() {
                                        sair = true;
                                    }
                                });
                            }
                            Tela::Migrando {
                                inicio,
                                copia,
                                etapa,
                                ..
                            } => {
                                componentes::titulo(ui, "Migrando o inventário", 20.0);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new());
                                    ui.label(format!("{etapa} · {} s", inicio.elapsed().as_secs()));
                                });
                                ui.label(
                                    RichText::new(format!("Cópia de antes: {}", copia.display()))
                                        .size(12.0)
                                        .color(p.tinta_suave),
                                );
                                ui.label(
                                    RichText::new(
                                        "Num inventário grande isso leva dezenas de segundos. Não feche a janela.",
                                    )
                                    .size(12.0)
                                    .color(p.tinta_suave),
                                );
                                ui.ctx().request_repaint_after(Duration::from_millis(200));
                            }
                            Tela::Pronto { aviso } => {
                                componentes::titulo(ui, "Inventário", 20.0);
                                ui.label(aviso.as_str());
                                ui.add_space(10.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Fechar").clicked() {
                                        sair = true;
                                    }
                                });
                            }
                        }
                    });
                });
            });
        if let Some(copia) = migrar {
            self.comecar_migracao(copia);
        }
        if sair {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
