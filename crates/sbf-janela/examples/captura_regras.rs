//! Capturas de tela da área Regras de reconhecimento, só com dados fictícios.
//!
//! `SBF_CENA=lista|pagina|pagina-exemplos|simulacao|reconhecimento-regra|reconhecimento-ambiguo|criar-regra \
//!  SBF_CAPTURA=/caminho/cena.png cargo run -p sbf-janela --example captura_regras`
//!
//! As três últimas cenas mostram o painel Reconhecimento falando com as regras (spec 13): o
//! comprovante de Pix reconhecido pela regra, o caso ambíguo e o formulário "Criar regra a
//! partir daqui". Tudo fictício: um catálogo em memória e arquivos de mentira na pasta temporária.

use std::io::Write;
use std::path::{Path, PathBuf};

use bancada_casca::Casca;
use bancada_casca::eframe;
use bancada_casca::egui;
use sbf_janela::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
use sbf_janela::reconhecimento_ficticio::{NOME_DO_AMBIGUO, NOME_DO_PIX};
use sbf_janela::regras::FonteDasRegras;
use sbf_janela::{AppSbf, Destino, EstadoDaBarra, Janela};

fn crc32(dados: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in dados {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let m = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & m);
        }
    }
    !crc
}

fn adler32(dados: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in dados {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn bloco(saida: &mut Vec<u8>, tipo: &[u8; 4], dados: &[u8]) {
    saida.extend_from_slice(&(dados.len() as u32).to_be_bytes());
    let mut c = tipo.to_vec();
    c.extend_from_slice(dados);
    saida.extend_from_slice(&c);
    saida.extend_from_slice(&crc32(&c).to_be_bytes());
}

fn salvar_png(caminho: &Path, largura: usize, altura: usize, rgba: &[u8]) -> std::io::Result<()> {
    let mut cru = Vec::with_capacity((largura * 4 + 1) * altura);
    for y in 0..altura {
        cru.push(0);
        cru.extend_from_slice(&rgba[y * largura * 4..(y + 1) * largura * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut pedacos = cru.chunks(65_535).peekable();
    while let Some(p) = pedacos.next() {
        z.push(u8::from(pedacos.peek().is_none()));
        let n = p.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(p);
    }
    z.extend_from_slice(&adler32(&cru).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(largura as u32).to_be_bytes());
    ihdr.extend_from_slice(&(altura as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    bloco(&mut png, b"IHDR", &ihdr);
    bloco(&mut png, b"IDAT", &z);
    bloco(&mut png, b"IEND", &[]);
    std::fs::File::create(caminho)?.write_all(&png)
}

struct App {
    casca: Casca<AppSbf>,
    cena: String,
    saida: PathBuf,
    quadros: u32,
    pedida: bool,
}

fn soltar(janela: &mut Janela, ctx: &egui::Context, nome: &str) {
    let arquivo = std::env::temp_dir().join(nome);
    let _ = std::fs::write(&arquivo, b"arquivo ficticio");
    janela.soltar_arquivos(vec![arquivo], Some(ctx));
}

impl App {
    /// As cenas do painel Reconhecimento: só ele no dock, e o arquivo solto no quadro 3.
    fn arrumar_o_reconhecimento(&mut self, ctx: &egui::Context) {
        let mut janela = self.casca.app.janela();
        match (self.quadros, self.cena.as_str()) {
            (1, _) => {
                bancada_casca::executar(ctx, bancada_casca::ids::painel("detalhe"));
                bancada_casca::executar(ctx, bancada_casca::ids::painel("previa"));
            }
            (3, "reconhecimento-ambiguo") => soltar(&mut janela, ctx, NOME_DO_AMBIGUO),
            (3, _) => soltar(&mut janela, ctx, NOME_DO_PIX),
            (8, "reconhecimento-regra") => {
                if let Some(c) = janela.reconhecimento.cartoes.first_mut() {
                    c.rastro_aberto = true;
                }
                janela.reconhecimento.rolar_para = Some(330.0);
            }
            (8, "criar-regra") => janela.abrir_criacao(0),
            (10, "criar-regra") => {
                if let Some(c) = janela.reconhecimento.criacao.as_mut() {
                    c.nome = "Pix para pessoas · Banco Exemplo".to_owned();
                    c.tipo_documental = "comprovante-pix".to_owned();
                    for l in &mut c.linhas {
                        match l.nome.as_str() {
                            "autenticacao" => l.texto = "D7K2-9QX1-EXEMPLO".to_owned(),
                            "vencimento" => l.texto = "20/11/2026".to_owned(),
                            _ => {}
                        }
                    }
                }
                janela.montar_criacao();
            }
            _ => {}
        }
    }

    fn arrumar(&mut self, ctx: &egui::Context) {
        if self.cena.starts_with("reconhecimento") || self.cena == "criar-regra" {
            self.arrumar_o_reconhecimento(ctx);
            return;
        }
        let mut janela = self.casca.app.janela();
        match self.quadros {
            1 => {
                let mut v = Vec::new();
                sbf_janela::navegacao::pedir_abrir(
                    ctx,
                    &mut v,
                    Destino::AreaFutura(sbf_janela::regras_ui::ROTA),
                    false,
                );
            }
            4 if self.cena != "lista" => {
                janela.regras.abrir("fatura-cartao-banco-exemplo");
                if self.cena == "simulacao" {
                    janela.regras.abrir("comprovante-pix-banco-exemplo");
                    janela.regras.selecionar_versao(1);
                    janela.regras.simular();
                }
            }
            8 if self.cena == "pagina-exemplos" => janela.regras.rolar_para = Some(420.0),
            8 if self.cena == "simulacao" => janela.regras.rolar_para = Some(700.0),
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.casca.desenhar(ui);
        self.quadros += 1;
        let ctx = ui.ctx().clone();
        ctx.request_repaint();
        self.arrumar(&ctx);
        if self.quadros >= 16 && !self.pedida {
            self.pedida = true;
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
            match salvar_png(&self.saida, img.size[0], img.size[1], &rgba) {
                Ok(()) => eprintln!("captura salva em {}", self.saida.display()),
                Err(e) => eprintln!("não deu para salvar a captura: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() -> std::process::ExitCode {
    let cena = std::env::var("SBF_CENA").unwrap_or_else(|_| "lista".into());
    let saida = std::env::var_os("SBF_CAPTURA").map_or_else(
        || PathBuf::from(format!("regras-{cena}.png")),
        PathBuf::from,
    );
    let (consulta, comandos) = sbf_janela::demonstracao::nova();
    let mut janela = Janela::nova();
    janela.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
    if let Some(hoje) = sbf_planejamento::Data::de_iso(HOJE_DA_DEMONSTRACAO) {
        janela.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje)));
    }
    match FonteDasRegras::ficticia(None) {
        Ok(f) => janela.regras.ligar(f),
        Err(e) => {
            eprintln!("o catálogo fictício não abriu: {e}");
            return std::process::ExitCode::FAILURE;
        }
    }
    sbf_janela::reconhecimento_ficticio::ligar_na_demonstracao(&mut janela);
    let barra = EstadoDaBarra {
        inventario: "Modo demonstração · dados fictícios".to_owned(),
        selo: Some("Demonstração · dados fictícios".to_owned()),
        ..Default::default()
    };
    let app = AppSbf::novo(janela, Box::new(consulta), Box::new(comandos), barra);
    let casca = match Casca::nova(app, sbf_janela::config_da_casca()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("os temas não carregaram: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let app = App {
        casca,
        cena,
        saida,
        quadros: 0,
        pedida: false,
    };
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Seu Bolso Feliz")
            .with_inner_size([1600.0, 1000.0]),
        ..Default::default()
    };
    let criar = Box::new(move |cc: &eframe::CreationContext<'_>| {
        sbf_janela::tema::instalar_fontes(&cc.egui_ctx);
        sbf_janela::tema::aplicar(&cc.egui_ctx, sbf_janela::tema::TEMA_PADRAO);
        Ok(Box::new(app) as Box<dyn eframe::App>)
    });
    match eframe::run_native("Seu Bolso Feliz", opcoes, criar) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("não deu para abrir a janela: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
