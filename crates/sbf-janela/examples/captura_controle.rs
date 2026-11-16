//! Capturas de tela do Controle mensal, só com dados fictícios.
//!
//! `SBF_CENA=ciclo|2050|pausada|editar|propostas|rubricas|documentos|anexar|novo|arrastar|conciliar|contas|faixa SBF_CAPTURA=/caminho/cena.png \
//!  xvfb-run -a cargo run -p sbf-janela --example captura_controle`
//!
//! Abre a janela da casca com o planejamento fictício ligado, arruma a cena, tira a foto da
//! janela em PNG e fecha. Nada aqui toca o cofre, o inventário ou a rede.

use std::io::Write;
use std::path::{Path, PathBuf};

use bancada_casca::Casca;
use bancada_casca::eframe;
use bancada_casca::egui;
use sbf_janela::controle::ModoDoPeriodo;
use sbf_janela::controle_ui::AbaDoDetalhe;
use sbf_janela::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
use sbf_janela::{AppSbf, EstadoDaBarra, Janela, Pedido, PedidoDoControle};
use sbf_planejamento::{Competencia, Data, Filtro, IdOcorrencia};

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

fn hoje() -> Data {
    Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
}

fn id(recorrente: &str, competencia: Competencia) -> IdOcorrencia {
    IdOcorrencia {
        recorrente: recorrente.into(),
        competencia,
        parcela: None,
    }
}

impl App {
    fn arrumar(&mut self, ctx: &egui::Context) {
        let h = Competencia::de_data(hoje());
        let mut janela = self.casca.app.janela();
        let c = &mut janela.controle;
        match (self.quadros, self.cena.as_str()) {
            (1, "ciclo") => {
                c.selecionada = Some(id("rec_internet", h));
            }
            (1, "2050") => {
                c.navegador.trocar_modo(ModoDoPeriodo::MesCivil);
                let _ = c.navegador.ir_para(Competencia { ano: 2050, mes: 1 });
            }
            (1, "pausada") => {
                c.navegador.proximo();
                c.navegador.proximo();
                c.resumo_aberto = false;
                c.filtro = Filtro::PausadosECancelados;
                c.selecionada = Some(id("rec_streaming", h.seguinte()));
                c.aba_do_detalhe = AbaDoDetalhe::Vigencia;
            }
            (1, "editar") => {
                c.selecionada = Some(id("rec_internet", h));
            }
            (4, "editar") => {
                janela.pedir(Pedido::Controle(PedidoDoControle::Acao(
                    sbf_janela::controle::AcaoDoControle::Editar,
                )));
            }
            (7, "editar") => {
                if let Some(d) = janela.controle.dialogo.as_mut()
                    && let sbf_janela::controle::Formulario::Editar { valor, escopo, .. } =
                        &mut d.formulario
                {
                    *valor = "139,90".into();
                    *escopo = sbf_janela::controle::EscopoEscolhido::DaquiEmDiante;
                }
            }
            (1, "abas") => c.resumo_aberto = false,
            (3, "abas") => {
                c.selecionar_recorrente("rec_salario");
                c.aba_do_detalhe = AbaDoDetalhe::Resumo;
            }
            (1, "rubricas") => c.resumo_aberto = true,
            // Só o Detalhe no dock: a tabela de rubricas pede altura.
            (2, "rubricas") => {
                bancada_casca::executar(ctx, bancada_casca::ids::painel("previa"));
            }
            (3, "rubricas") => {
                c.selecionar_recorrente("rec_salario");
                c.aba_do_detalhe = AbaDoDetalhe::Rubricas;
            }
            (1, "documentos" | "anexar") => c.resumo_aberto = false,
            (3, "documentos" | "anexar") => {
                c.selecionar_recorrente("rec_energia");
                c.aba_do_detalhe = AbaDoDetalhe::Documentos;
            }
            (4, "anexar") => {
                janela.pedir(Pedido::Controle(PedidoDoControle::Acao(
                    sbf_janela::controle::AcaoDoControle::AnexarDocumento,
                )));
            }
            (1, "novo") => c.resumo_aberto = false,
            (3, "novo") => c.selecionar_recorrente("rec_aluguel"),
            (4, "novo") => {
                janela.pedir(Pedido::Controle(PedidoDoControle::Acao(
                    sbf_janela::controle::AcaoDoControle::AdicionarCompromisso,
                )));
            }
            (7, "novo") => {
                if let Some(d) = janela.controle.novo.as_mut() {
                    use sbf_janela::compromisso::FrequenciaEscolhida;
                    let f = &mut d.formulario;
                    f.nome = "Presente da Ana".into();
                    f.subgrupo = "Eventos".into();
                    f.valor = "250,00".into();
                    f.estimado = true;
                    f.frequencia = FrequenciaEscolhida::Unica;
                    f.data_unica = "15/02/2027".into();
                    f.conta = "Nubank".into();
                    f.centro = "Pessoal".into();
                }
            }
            (1, "arrastar" | "arrastar-fim") => {
                c.resumo_aberto = false;
                // Só o painel Reconhecimento no dock: o cartão pede altura.
                bancada_casca::executar(ctx, bancada_casca::ids::painel("detalhe"));
                bancada_casca::executar(ctx, bancada_casca::ids::painel("previa"));
            }
            (3, "arrastar" | "arrastar-fim") => {
                let arquivo = std::env::temp_dir().join("contracheque-2026-10.pdf");
                let _ = std::fs::write(&arquivo, b"arquivo ficticio");
                janela.soltar_arquivos(vec![arquivo], Some(ctx));
            }
            (9, "arrastar" | "arrastar-fim") => {
                // O usuário confirma a primeira sugestão (vincular); a segunda fica à espera.
                janela.confirmar_sugestao(0, 0);
            }
            (11, "arrastar") => {
                // Rola até os grupos de rubricas da sugestão "usar estas rubricas".
                janela.reconhecimento.rolar_para = Some(RECONHECIMENTO_ROLAGEM);
            }
            (11, "arrastar-fim") => {
                // O fim do cartão: só deste mês, fora do total, líquidos e a divergência.
                janela.reconhecimento.rolar_para = Some(RECONHECIMENTO_ROLAGEM_FIM);
            }
            // O Detalhe do Salário de novembro (no ciclo de hoje): a sugestão do extrato, o selo
            // das três provas (com o par de conta confirmado) e a lista "Conciliar com outro…".
            (1, "conciliar") => {
                c.navegador.ir_para_hoje();
                c.resumo_aberto = false;
                c.selecionada = Some(id("rec_salario", h));
                c.aba_do_detalhe = AbaDoDetalhe::Resumo;
                c.conciliar_aberto = Some(id("rec_salario", h));
            }
            (2, "conciliar" | "faixa") => {
                bancada_casca::executar(ctx, bancada_casca::ids::painel("previa"));
            }
            (3, "conciliar") => {
                janela.pedir(Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
                    sbf_planejamento::Alteracao::MapearConta {
                        apelido: "CX-0630".into(),
                        conta_do_extrato: Some(
                            sbf_janela::planejamento_ficticio::CONTA_DO_SALARIO.into(),
                        ),
                    },
                ])));
            }
            (1, "contas") => {
                let mut v = Vec::new();
                sbf_janela::navegacao::pedir_abrir(
                    ctx,
                    &mut v,
                    sbf_janela::Destino::AreaFutura(sbf_janela::contas_ui::ROTA),
                    false,
                );
            }
            // O contracheque de dezembro, vinculado, tem rubricas que a composição não usa.
            (1, "faixa") => {
                c.navegador.ir_para_hoje();
                c.resumo_aberto = false;
                c.selecionada = Some(id("rec_salario", h));
                c.aba_do_detalhe = AbaDoDetalhe::Rubricas;
            }
            (3, "faixa") => {
                janela.pedir(Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
                    sbf_planejamento::Alteracao::VincularDocumento {
                        alvo: sbf_planejamento::AlvoDoDocumento::Recorrente {
                            id: "rec_salario".into(),
                        },
                        sha256: "sha-do-contracheque-2026-12".into(),
                        tipo_documento: sbf_planejamento::TipoDeDocumento::Contracheque,
                        nome: "contracheque-2026-12.pdf".into(),
                        data: Data::de_iso("2026-12-05"),
                    },
                ])));
            }
            (1, "propostas") => {
                c.resumo_aberto = false;
                c.abrir_lateral(sbf_janela::controle_ui::AbaLateral::Propostas);
            }
            _ => {}
        }
        drop(janela);
        let _ = ctx;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.casca.desenhar(ui);
        self.quadros += 1;
        let ctx = ui.ctx().clone();
        ctx.request_repaint();
        self.arrumar(&ctx);
        if self.quadros >= 14 && !self.pedida {
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

/// Quanto rolar o painel do reconhecimento para mostrar as rubricas.
const RECONHECIMENTO_ROLAGEM: f32 = 640.0;

/// Idem, até o fim do cartão (a rolagem passa do fim sem problema: o egui limita).
const RECONHECIMENTO_ROLAGEM_FIM: f32 = 5000.0;

fn main() -> std::process::ExitCode {
    let cena = std::env::var("SBF_CENA").unwrap_or_else(|_| "ciclo".into());
    let saida = std::env::var_os("SBF_CAPTURA").map_or_else(
        || PathBuf::from(format!("controle-{cena}.png")),
        PathBuf::from,
    );
    let (consulta, comandos) = sbf_janela::demonstracao::nova();
    let mut janela = Janela::nova();
    janela.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
    janela.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
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
