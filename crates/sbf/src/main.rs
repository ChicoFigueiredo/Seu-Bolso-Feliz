//! O binário do Seu Bolso Feliz desktop (ADR-012). Na fatia 1, só o modo janela:
//!
//! - `sbf --demonstracao`: a janela com dados fictícios em memória;
//! - `sbf --inventario <pasta-do-cofre>`: o inventário de verdade (o integrador liga o
//!   `LeitorSqlite` e o `Inventario`; até lá, uma mensagem clara);
//! - `sbf --versao`.

mod captura;

use std::path::PathBuf;
use std::process::ExitCode;

use sbf_inventario::{Comandos, Consulta};
use sbf_janela::{EstadoDaBarra, Janela, MolduraProvisoria};

struct App {
    moldura: MolduraProvisoria,
    consulta: Box<dyn Consulta>,
    comandos: Box<dyn Comandos>,
    captura: Option<PathBuf>,
    quadros: u32,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let inicio = std::time::Instant::now();
        self.moldura
            .ui(ui, self.consulta.as_ref(), self.comandos.as_mut());
        if std::env::var_os("SBF_DEPURAR").is_some() {
            eprintln!("quadro em {:?}", inicio.elapsed());
        }
        let Some(caminho) = self.captura.clone() else {
            return;
        };
        self.quadros += 1;
        let ctx = ui.ctx().clone();
        ctx.request_repaint();
        if self.quadros == 1
            && let Ok(alvo) = std::env::var("SBF_CAPTURA_ABRIR")
        {
            abrir_para_captura(&mut self.moldura, &alvo);
        }
        if self.quadros == 8 {
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

fn abrir_para_captura(moldura: &mut MolduraProvisoria, alvo: &str) {
    use sbf_inventario::consulta::Area;
    use sbf_janela::{Destino, Pedido};
    let destino = match alvo.strip_prefix("area:") {
        Some(a) => sbf_janela::areas::areas_da_fatia()
            .into_iter()
            .find(|x| format!("{x:?}") == a)
            .map(Destino::Area),
        None => alvo.parse().ok().map(Destino::Item),
    };
    let destino = destino.unwrap_or(Destino::Area(Area::VisaoGeral));
    moldura.janela.pedir(Pedido::Abrir(destino));
}

fn abrir_janela(app: App) -> ExitCode {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Seu Bolso Feliz")
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    let resultado = eframe::run_native(
        "Seu Bolso Feliz",
        opcoes,
        Box::new(|cc| {
            app.moldura.preparar(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
    match resultado {
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
            abrir_janela(App {
                moldura: MolduraProvisoria::nova(Janela::nova(), barra),
                consulta: Box::new(consulta),
                comandos: Box::new(comandos),
                captura: std::env::var_os("SBF_CAPTURA").map(PathBuf::from),
                quadros: 0,
            })
        }
        Some("--inventario") => {
            let Some(pasta) = args.get(1) else {
                eprintln!("sbf: falta a pasta do cofre.\n{AJUDA}");
                return ExitCode::from(2);
            };
            eprintln!(
                "sbf: o modo inventário ainda não está ligado nesta versão.\n\
                 A pasta {pasta} não foi aberta nem tocada.\n\
                 A leitura (LeitorSqlite) e os comandos (Inventario) estão sendo ligados à janela; \
                 até lá, use `sbf --demonstracao` para ver a janela com dados fictícios."
            );
            ExitCode::from(3)
        }
        _ => {
            eprintln!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
