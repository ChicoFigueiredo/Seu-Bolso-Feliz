//! O binário do Seu Bolso Feliz desktop (ADR-012). Na fatia 1, só o modo janela:
//!
//! - `sbf --demonstracao`: a janela com dados fictícios em memória;
//! - `sbf --inventario <pasta-do-cofre>`: o inventário de verdade (`LeitorSqlite` e
//!   `Inventario`), com o plano de migração antes (só migra com o OK);
//! - `sbf --versao`.

mod captura;
mod inventario;

use std::path::PathBuf;
use std::process::ExitCode;

use bancada_casca::Casca;
use sbf_inventario::{Comandos, Consulta};
use sbf_janela::{AppSbf, EstadoDaBarra, Janela};

/// A janela: o app do SBF dentro da `bancada-casca`, mais a captura de tela opcional.
struct App {
    casca: Casca<AppSbf>,
    captura: Option<PathBuf>,
    quadros: u32,
}

impl App {
    fn novo(
        janela: Janela,
        consulta: Box<dyn Consulta>,
        comandos: Box<dyn Comandos>,
        barra: EstadoDaBarra,
    ) -> Result<Self, String> {
        let app = AppSbf::novo(janela, consulta, comandos, barra);
        let mut config = sbf_janela::config_da_casca();
        if let Ok(t) = std::env::var("SBF_TEMA") {
            config.tema_inicial = t;
        }
        let casca = Casca::nova(app, config).map_err(|e| e.to_string())?;
        Ok(Self {
            casca,
            captura: std::env::var_os("SBF_CAPTURA").map(PathBuf::from),
            quadros: 0,
        })
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let inicio = std::time::Instant::now();
        self.casca.desenhar(ui);
        if std::env::var_os("SBF_DEPURAR").is_some() {
            eprintln!("quadro em {:?}", inicio.elapsed());
        }
        let Some(caminho) = self.captura.clone() else {
            return;
        };
        self.quadros += 1;
        let ctx = ui.ctx().clone();
        ctx.request_repaint();
        if self.quadros == 1 {
            preparar_captura(&mut self.casca, &ctx);
        }
        if self.quadros == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let imagem = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = imagem {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            match captura::salvar_png(&caminho, img.size[0], img.size[1], &rgba) {
                Ok(()) => eprintln!("sbf: captura salva em {}", caminho.display()),
                Err(e) => eprintln!("sbf: não deu para salvar a captura: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// `SBF_CAPTURA_ABRIR` (uma rota: `a-triar`, `fornecedor:Energia Exemplo`…),
/// `SBF_CAPTURA_ABA` (o índice da aba da página) e `SBF_CENA` (`paleta`, `keytips`,
/// `simples`, `bastidores`, `flutuante`).
fn preparar_captura(casca: &mut Casca<AppSbf>, ctx: &egui::Context) {
    use sbf_janela::Destino;
    if let Ok(rota) = std::env::var("SBF_CAPTURA_ABRIR") {
        if let (Some(Destino::Item(r)), Some(aba)) = (
            Destino::da_rota(&rota),
            std::env::var("SBF_CAPTURA_ABA")
                .ok()
                .and_then(|a| a.parse().ok()),
        ) {
            casca.app.janela().centro.abas.insert(r, aba);
        }
        bancada_casca::abrir(ctx, rota);
    }
    match std::env::var("SBF_CENA").unwrap_or_default().as_str() {
        "paleta" => {
            casca.estado.paleta.abrir();
            casca.estado.paleta.busca = "ener".into();
        }
        "keytips" => casca.mostrar_keytips(),
        "simples" => casca
            .estado
            .ribbon
            .definir_modo(bancada_casca::ModoRibbon::Simples),
        "bastidores" => {
            casca.estado.ribbon.bastidores_abertos = true;
            casca.estado.secao_bastidores = Some("opcoes".into());
        }
        "flutuante" => {
            bancada_casca::abrir_flutuante(ctx, "fornecedor:Água Sigma");
        }
        _ => {}
    }
}

fn abrir_janela_de(app: Box<dyn eframe::App>) -> ExitCode {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Seu Bolso Feliz")
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    let criar = Box::new(|cc: &eframe::CreationContext<'_>| {
        // As fontes valem do quadro seguinte em diante: instale antes do primeiro.
        sbf_janela::tema::instalar_fontes(&cc.egui_ctx);
        sbf_janela::tema::aplicar(&cc.egui_ctx, sbf_janela::tema::TEMA_PADRAO);
        Ok(app)
    });
    match eframe::run_native("Seu Bolso Feliz", opcoes, criar) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf: não deu para abrir a janela: {e}");
            ExitCode::FAILURE
        }
    }
}

const AJUDA: &str = "uso: sbf --demonstracao | sbf --inventario <pasta-do-cofre> | sbf --versao";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--versao") | Some("--version") => {
            println!("sbf {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--demonstracao") => {
            let (consulta, comandos) = sbf_janela::demonstracao::nova();
            let barra = EstadoDaBarra {
                inventario: "Modo demonstração · dados fictícios".to_owned(),
                selo: Some("Demonstração · dados fictícios".to_owned()),
                ..Default::default()
            };
            match App::novo(
                Janela::nova(),
                Box::new(consulta),
                Box::new(comandos),
                barra,
            ) {
                Ok(app) => abrir_janela_de(Box::new(app)),
                Err(e) => {
                    eprintln!("sbf: os temas não carregaram: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("--inventario") => {
            let Some(pasta) = args.get(1) else {
                eprintln!("sbf: falta a pasta do cofre.\n{AJUDA}");
                return ExitCode::from(2);
            };
            match inventario::abrir(std::path::Path::new(pasta)) {
                Ok(app) => abrir_janela_de(Box::new(app)),
                Err(e) => {
                    eprintln!("sbf: não deu para abrir o inventário: {e}");
                    ExitCode::from(3)
                }
            }
        }
        _ => {
            eprintln!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
