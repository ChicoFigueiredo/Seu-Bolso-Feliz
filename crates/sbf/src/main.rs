//! O binário do Seu Bolso Feliz desktop (ADR-012). Na fatia 1, só o modo janela:
//!
//! - `sbf`: a tela de entrada (abrir a pasta do cofre ou o modo demonstração);
//! - `sbf --demonstracao`: a janela com dados fictícios em memória;
//! - `sbf --inventario <pasta-do-cofre>`: o inventário de verdade (`LeitorSqlite` e
//!   `Inventario`), com o plano de migração antes (só migra com o OK);
//! - `sbf reprojetar --inventario <pasta-do-cofre>`: manutenção, refaz a projeção a partir
//!   do log de decisões e imprime o resumo;
//! - `sbf --versao`.

mod captura;
mod entrada;
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
    captura_pedida: bool,
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
            captura_pedida: false,
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
        // O foco depois da navegação (abrir uma área limpa a seleção).
        if self.quadros == 4 {
            focar_para_captura(&mut self.casca, &ctx);
        }
        // A prévia carrega numa thread: espera ela terminar (até uns 10 s).
        let previa_ocupada = self.casca.app.janela().previas.ocupada();
        if self.quadros >= 10 && !self.captura_pedida && (!previa_ocupada || self.quadros > 600) {
            self.captura_pedida = true;
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
/// `SBF_CAPTURA_ABA` (o índice da aba da página), `SBF_CAPTURA_FOCO` (a rota do item em
/// foco no dock: a prévia dele no painel) e `SBF_CENA` (`paleta`, `keytips`, `simples`,
/// `bastidores`, `flutuante`, `previa-destacada`).
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
        "regras" => bancada_casca::executar(ctx, "ferramentas.regras-de-senha"),
        "mudar-categoria" => bancada_casca::executar(ctx, "livro.mudar-categoria"),
        "pdf" => bancada_casca::executar(ctx, "ferramentas.pdf-protegido"),
        "chaveiro" => {
            casca.estado.ribbon.bastidores_abertos = true;
            casca.estado.secao_bastidores = Some("chaveiro".into());
        }
        "flutuante" => {
            bancada_casca::abrir_flutuante(ctx, "fornecedor:Água Sigma");
        }
        _ => {}
    }
}

/// `SBF_CAPTURA_FOCO`: o item em foco no dock (a prévia dele no canto), com as cenas
/// `previa-destacada` e `previa-dica`.
fn focar_para_captura(casca: &mut Casca<AppSbf>, ctx: &egui::Context) {
    use sbf_janela::Destino;
    if let Ok(rota) = std::env::var("SBF_CAPTURA_FOCO")
        && let Some(Destino::Item(r)) = Destino::da_rota(&rota)
    {
        if std::env::var("SBF_CENA").as_deref() == Ok("previa-destacada") {
            // A captura só pega a janela principal: a destacada aparece embutida.
            ctx.set_embed_viewports(true);
            casca.app.janela().previas.destacar(&r);
        }
        if std::env::var("SBF_CENA").as_deref() == Ok("previa-dica") {
            let perto =
                egui::Rect::from_min_size(egui::pos2(260.0, 300.0), egui::vec2(180.0, 18.0));
            casca.app.janela().previas.fixar_dica(r.clone(), perto);
        }
        casca.app.janela().centro.selecao.foco = Some(r);
    }
}

fn abrir_janela_de(app: Box<dyn eframe::App>) -> ExitCode {
    // As sobras da conversão do Office de uma sessão interrompida (pasta 0700 em tmpfs).
    sbf_janela::previa::faxinar_sobras_em_segundo_plano();
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

/// A janela do modo demonstração, com dados fictícios em memória.
fn app_de_demonstracao() -> Result<App, String> {
    let (consulta, comandos) = sbf_janela::demonstracao::nova();
    let barra = EstadoDaBarra {
        inventario: "Modo demonstração · dados fictícios".to_owned(),
        selo: Some("Demonstração · dados fictícios".to_owned()),
        ..Default::default()
    };
    let mut janela = Janela::nova();
    // A prévia precisa de arquivos: fictícios, numa pasta temporária apagada ao sair.
    if let Err(e) = janela.previas.preparar_demonstracao(&consulta) {
        eprintln!("sbf: {e}");
    }
    // O livro-caixa fictício (fatia 2): a mesma leitura e a estação de mentira.
    let livro = sbf_janela::livro_ficticio::novo();
    janela.hoje = sbf_janela::livro_ficticio::HOJE.to_owned();
    match std::env::var("SBF_CENA").as_deref() {
        Ok("offline") => livro.definir_rede(sbf_inventario::alteracao::EstadoDaRede::SemRede),
        Ok("revogada") => {
            livro.definir_rede(sbf_inventario::alteracao::EstadoDaRede::EstacaoRevogada)
        }
        Ok("regras-novas") => {
            livro.definir_rede(sbf_inventario::alteracao::EstadoDaRede::RegrasMaisNovas {
                versao_do_servidor: "0.2.0".to_owned(),
            })
        }
        _ => {}
    }
    janela.ligar_estacao(Box::new(livro.clone()));
    let consulta = sbf_janela::ConsultaComposta::nova(Box::new(consulta), Some(Box::new(livro)));
    App::novo(janela, Box::new(consulta), Box::new(comandos), barra)
        .map_err(|e| format!("os temas não carregaram: {e}"))
}

const AJUDA: &str = "uso: sbf | sbf --demonstracao | sbf --inventario <pasta-do-cofre> | sbf reprojetar --inventario <pasta-do-cofre> | sbf --versao";

/// Gancho de pânico (achado A16 da revisão de segurança): mostra uma mensagem genérica e só
/// o local (arquivo e linha). **Nunca o payload**, que pode carregar trecho de PDF ou outro
/// dado do inventário.
fn instalar_gancho_de_panico() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{}", texto_do_panico(info.location()));
    }));
}

fn texto_do_panico(local: Option<&std::panic::Location<'_>>) -> String {
    match local {
        Some(l) => format!(
            "sbf: falha interna em {}:{}. A janela pode ter fechado; abra de novo. Os detalhes não são mostrados porque podem conter dados.",
            l.file(),
            l.line()
        ),
        None => "sbf: falha interna. Os detalhes não são mostrados porque podem conter dados."
            .to_owned(),
    }
}

fn main() -> ExitCode {
    instalar_gancho_de_panico();
    // Só para o teste do gancho: um pânico com um payload que não pode aparecer.
    if std::env::var_os("SBF_TESTE_DO_GANCHO_DE_PANICO").is_some() {
        let payload = String::from("payload-que-nao-pode-aparecer");
        std::panic::panic_any(payload);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--versao") | Some("--version") => {
            println!("sbf {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--demonstracao") => match app_de_demonstracao() {
            Ok(app) => abrir_janela_de(Box::new(app)),
            Err(e) => {
                eprintln!("sbf: {e}");
                ExitCode::FAILURE
            }
        },
        None => abrir_janela_de(Box::new(entrada::Entrada::nova())),
        Some("reprojetar") => {
            let (Some("--inventario"), Some(pasta)) =
                (args.get(1).map(String::as_str), args.get(2))
            else {
                eprintln!("sbf: uso: sbf reprojetar --inventario <pasta-do-cofre>\n{AJUDA}");
                return ExitCode::from(2);
            };
            match sbf_inventario::manutencao::reprojetar_a_pasta(std::path::Path::new(pasta)) {
                Ok(r) => {
                    println!("{}", r.resumo());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("sbf: {e}");
                    ExitCode::from(1)
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
