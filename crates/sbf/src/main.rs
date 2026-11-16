//! O binário do Seu Bolso Feliz desktop (ADR-012). Na fatia 1, só o modo janela:
//!
//! - `sbf`: a tela de entrada (abrir a pasta do cofre ou o modo demonstração);
//! - `sbf --demonstracao`: a janela com dados fictícios em memória;
//! - `sbf --inventario <pasta-do-cofre>`: o inventário de verdade (`LeitorSqlite` e
//!   `Inventario`), com o plano de migração antes (só migra com o OK);
//! - `sbf reprojetar --inventario <pasta-do-cofre>`: manutenção, refaz a projeção a partir
//!   do log de decisões e imprime o resumo;
//! - `sbf mcp --inventario <pasta-do-cofre>`: o servidor MCP local por stdio (ver `mcp.rs`);
//! - `sbf worker rodar|instalar|desinstalar|estado`: o worker local sem janela (`worker.rs`);
//! - `sbf servidor`: a API do livro-caixa na VPS (fatia 2; ver `servidor.rs`);
//! - `sbf coletor-vps`: as coletas da VPS, Gmail e Pluggy (ADR-014; ver `coletor.rs`);
//! - `sbf canais`: o bot do Telegram e os alertas, na VPS (ADR-014, ADR-017; ver `canais.rs`);
//! - `sbf reconhecer --inventario <cofre> --arquivo <caminho>`: o reconhecimento de um arquivo
//!   solto, sem janela (ver `reconhecer.rs`);
//! - `sbf --versao`.

mod caixa;
mod canais;
mod captura;
mod coletor;
mod entrada;
mod estacao;
mod inventario;
mod mcp;
mod planejamento;
mod rag;
mod reconhecer;
mod roteiros;
mod servidor;
mod triagem;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use bancada_casca::Casca;
use sbf_inventario::{Comandos, Consulta};
use sbf_janela::{AppSbf, EstadoDaBarra, Janela};

/// A janela: o app do SBF dentro da `bancada-casca`, mais a captura de tela opcional.
struct App {
    casca: Casca<AppSbf>,
    /// A estação do livro-caixa (`--inventario` e sem argumento), ticada a cada quadro.
    estacao: Option<estacao::Ligacao>,
    captura: Option<PathBuf>,
    quadros: u32,
    captura_pedida: bool,
    /// A barra de status mostra o estado do worker (só no inventário real).
    mostrar_worker: bool,
    worker_lido: Option<std::time::Instant>,
    /// A cena da demonstração: a rota a abrir e os pedidos a fazer no primeiro quadro.
    cena: Option<(String, Vec<sbf_janela::Pedido>)>,
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
            estacao: None,
            captura: std::env::var_os("SBF_CAPTURA").map(PathBuf::from),
            quadros: 0,
            captura_pedida: false,
            cena: None,
            mostrar_worker: false,
            worker_lido: None,
        })
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let inicio = std::time::Instant::now();
        if let Some(estacao) = self.estacao.as_mut() {
            if estacao.tique() {
                // Uma troca terminou ou houve escrita local: os números e as tabelas
                // guardados valem pela cópia anterior.
                self.casca.app.janela().reler();
            }
            // O intervalo e o fim da troca precisam de quadros mesmo sem o mouse.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(
                    if estacao.sincronizando() { 200 } else { 1000 },
                ));
        }
        if self.mostrar_worker
            && self
                .worker_lido
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(3))
        {
            // O worker escreve um arquivo de estado; a janela só o lê (não bloqueia).
            self.casca.app.barra.worker = Some(worker::texto_da_barra_do_worker());
            self.worker_lido = Some(std::time::Instant::now());
        }
        if self.mostrar_worker {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(3));
        }
        self.casca.desenhar(ui);
        if let Some(p) = self.casca.app.tomar_preferencias_da_casca() {
            self.casca.aplicar_preferencias(&p);
        }
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
            if let Some((rota, pedidos)) = self.cena.take() {
                bancada_casca::abrir(&ctx, rota);
                for p in pedidos {
                    self.casca.app.janela().pedir(p);
                }
            }
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
/// `bastidores`, `chaves-ia`, `flutuante`, `previa-destacada`, `perguntar`).
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
        "chaves-ia" => {
            casca.estado.ribbon.bastidores_abertos = true;
            casca.estado.secao_bastidores = Some("opcoes".into());
            casca.app.janela().triagem.chaves.rolar_ate_aqui = 6;
        }
        // Perguntar já respondida (a demonstração usa o provedor falso e o índice em memória).
        "perguntar" => {
            casca
                .app
                .janela()
                .perguntar
                .perguntar(ctx, "Quanto foi a fatura de energia elétrica de outubro?");
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
    // A caixa de entrada fictícia (fatia 3): rascunhos ligados aos documentos do acervo e as
    // movimentações do livro como base de edição.
    let documentos: Vec<sbf_inventario::Ref> = sbf_inventario::Consulta::tabela(
        &consulta,
        &sbf_inventario::consulta::PedidoDeTabela::Area {
            area: sbf_inventario::consulta::Area::Documentos,
            filtro: sbf_inventario::consulta::Filtro::default(),
        },
    )
    .map(|t| {
        t.linhas
            .into_iter()
            .filter_map(|l| l.alvo)
            .take(5)
            .collect()
    })
    .unwrap_or_default();
    let caixa = sbf_janela::caixa::demonstracao(&documentos, Box::new(livro.clone()));
    janela.ligar_caixa(Box::new(caixa.clone()));
    // Roteiros fictícios: nenhum Chrome, nenhum site.
    janela
        .roteiros
        .demonstracao(&std::env::var("SBF_CENA").unwrap_or_default());
    // A triagem por IA fictícia: dados inventados, nenhum provedor é chamado.
    janela.ligar_triagem(Box::new(sbf_triagem::servico::TriagemFicticia::nova()));
    // Perguntar e Pontos de atenção fictícios: motor verdadeiro, dados inventados.
    janela.ligar_perguntar(sbf_janela::analise_ficticia::perguntar_da_demonstracao(
        &documentos,
    ));
    janela.ligar_pontos(sbf_janela::analise_ficticia::pontos_da_demonstracao(
        &documentos,
    ));
    // As chaves da demonstração: uma guarda em memória com uma chave inventada.
    janela.ligar_chaves_de_ia(
        Some(Box::new(sbf_janela::chaves_ia::GuardaFicticia::com_chave(
            "openrouter",
            "sk-or-v1-ficticia-0000a1b2",
        ))),
        Some(std::path::PathBuf::from("/home/exemplo/.local/bin/claude")),
    );
    let cena = preparar_cena_da_caixa(&caixa, &consulta, &livro, &mut janela);
    let consulta = sbf_janela::ConsultaComposta::nova(Box::new(consulta), Some(Box::new(livro)));
    let mut app = App::novo(janela, Box::new(consulta), Box::new(comandos), barra)
        .map_err(|e| format!("os temas não carregaram: {e}"))?;
    app.cena = cena;
    Ok(app)
}

/// A primeira linha com alvo de uma área (para as cenas de captura).
fn primeiro_alvo(
    c: &dyn Consulta,
    area: sbf_inventario::consulta::Area,
) -> Option<sbf_inventario::Ref> {
    c.tabela(&sbf_inventario::consulta::PedidoDeTabela::Area {
        area,
        filtro: sbf_inventario::consulta::Filtro::default(),
    })
    .ok()?
    .linhas
    .into_iter()
    .find_map(|l| l.alvo)
}

/// `SBF_CENA` da caixa: deixa a demonstração num estado que mostra pendente, aceita e recusada
/// (`caixa-pendente`, `caixa-recusa`, `caixa-sinc`, `caixa-mov`, `caixa-edicao`, `caixa-rateio`,
/// `caixa-exclusao`, `caixa-envio`). Devolve a rota a abrir e os pedidos do primeiro quadro.
fn preparar_cena_da_caixa(
    caixa: &sbf_janela::caixa::CaixaFicticia,
    inventario: &dyn Consulta,
    livro: &sbf_janela::livro_ficticio::LivroFicticio,
    janela: &mut Janela,
) -> Option<(String, Vec<sbf_janela::Pedido>)> {
    use sbf_inventario::alteracao::CampoLadoALado;
    use sbf_inventario::consulta::Area;
    use sbf_janela::Pedido;
    use sbf_janela::caixa::{Editor, FonteDaCaixa};
    use sbf_janela::caixa_ui::PedidoDaCaixa as P;
    let cena = std::env::var("SBF_CENA").unwrap_or_default();
    let mut c = caixa.clone();
    match cena.as_str() {
        "caixa-pendente" | "caixa-recusa" | "caixa-sinc" => {
            let _ = c.aprovar("r1");
            let _ = c.aprovar("r3");
            caixa.servidor_aceita_tudo();
            let _ = c.aprovar("r4");
            if cena != "caixa-pendente"
                && let Ok(fila) = c.escritas()
                && let Some(e) = fila.first()
            {
                caixa.servidor_recusa(
                    &e.id,
                    "O rascunho mudou no servidor depois que você aprovou.",
                    vec![
                        CampoLadoALado::novo("Valor", "R$ 458,10", "R$ 460,00"),
                        CampoLadoALado::novo(
                            "Conta ou cartão",
                            "Cartão Azul",
                            "Conta corrente Exemplo",
                        ),
                        CampoLadoALado::novo("Fornecedor", "Mercado Delta", "Mercado Delta"),
                    ],
                );
            }
            if cena == "caixa-sinc" {
                // Mais uma na fila, esperando.
                let _ = c.aprovar("r5");
                let _ = c.aprovar("r2");
                return Some(("sincronizacao".to_owned(), Vec::new()));
            }
            None
        }
        "caixa-mov" | "caixa-edicao" | "caixa-rateio" | "caixa-exclusao" => {
            let mov = primeiro_alvo(livro, Area::Movimentacoes)?;
            let m = c.movimentacao(&mov).ok().flatten()?;
            if cena == "caixa-mov" {
                let mut campos = m.campos.clone();
                campos.descricao = format!("{} (conferida)", campos.descricao);
                let _ = c.editar_movimentacao(&mov, campos);
            }
            let pedido = match cena.as_str() {
                "caixa-edicao" => Some(P::PedirEdicao(mov.clone())),
                "caixa-rateio" => Some(P::PedirRateio(mov.clone())),
                "caixa-exclusao" => Some(P::PedirExclusao(mov.clone())),
                _ => None,
            };
            Some((
                mov.to_string(),
                pedido.map(|p| vec![Pedido::Caixa(p)]).unwrap_or_default(),
            ))
        }
        "caixa-envio" => {
            let lote = primeiro_alvo(inventario, Area::Lotes)?;
            janela.lote_atual = Some(lote.clone());
            let _ = c.enviar_lote(&lote);
            caixa.avancar_envio();
            caixa.avancar_envio();
            Some(("lote".to_owned(), Vec::new()))
        }
        _ => None,
    }
}

const AJUDA: &str = "uso: sbf | sbf --demonstracao | sbf --inventario <pasta-do-cofre> | sbf reprojetar --inventario <pasta-do-cofre> | sbf mcp --inventario <pasta-do-cofre> | sbf planejamento importar|ocultos|conciliar --inventario <pasta-do-cofre> | sbf reconhecer --inventario <pasta-do-cofre> --arquivo <caminho> | sbf servidor [--endereco 127.0.0.1:PORTA] | sbf worker rodar|instalar|desinstalar|estado | sbf coletor-vps [gmail|pluggy|comparar] | sbf canais [vincular-telegram|alertas|nivel|desvincular] | sbf estacao importar-credencial [--servidor <url>] | sbf --versao";

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
        Some("servidor") => servidor::rodar(&args[1..]),
        Some("coletor-vps") => coletor::rodar(&args[1..]),
        Some("worker") => worker::rodar(&args[1..]),
        Some("canais") => canais::rodar(&args[1..]),
        Some("mcp") => mcp::rodar(&args[1..]),
        Some("planejamento") => planejamento::rodar(&args[1..]),
        Some("reconhecer") => reconhecer::rodar(&args[1..]),
        Some("estacao") => estacao::importar_credencial_cli(&args[1..]),
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
