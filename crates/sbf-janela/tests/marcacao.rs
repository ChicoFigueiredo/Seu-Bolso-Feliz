#![allow(clippy::unwrap_used)]
//! A marcação das listas de documentos (spec 15, RITA-01): a Caixa de entrada, a seção
//! "Sem padrão de reconhecimento" de A triar e a tabela de achados pintam de verde o que um
//! padrão casou, de âmbar o ambíguo e deixam sem cor o resto; o selo conta os padrões e o
//! filtro separa os três grupos. Desativar o padrão mantém o verde. Tudo com dados fictícios
//! e o catálogo de padrões em memória.

use std::sync::Arc;

use bancada_casca::Casca;
use sbf_inventario::Consulta as _;
use sbf_inventario::consulta::{Area, Filtro, PedidoDeTabela};
use sbf_janela::caixa_de_entrada::CaixaDeEntradaFicticia;
use sbf_janela::marcacao::{CorDaMarca, FiltroDeCor};
use sbf_janela::reconhecimento_ficticio::{
    NOME_DO_AMBIGUO, NOME_DO_PIX, NOME_DO_RECIBO_SEM_PADRAO, reconhecedor_de_demonstracao,
};
use sbf_janela::regras::FonteDasRegras;
use sbf_janela::{AppSbf, Destino, EstadoDaBarra, Janela, demonstracao};
use sbf_regras::{Decisao, Execucao, Leitura, ResultadoDaExecucao};

struct Banco {
    ctx: egui::Context,
    casca: Casca<AppSbf>,
    regras: FonteDasRegras,
    /// Os dois primeiros achados da tabela de A triar da demonstração.
    achados: Vec<String>,
}

fn sha(nome: &str) -> String {
    sbf_janela::demonstracao::hash_do_nome(nome)
}

fn execucao(regra: &str) -> Execucao {
    Execucao {
        regra: regra.to_owned(),
        versao: 1,
        resultado: ResultadoDaExecucao::Reconhecido,
        motivo: None,
        detalhe: None,
        duracao_us: 1,
    }
}

fn casou(entrada: &str, regras: &[&str]) -> Leitura {
    Leitura {
        entrada: entrada.to_owned(),
        decisao: Decisao::Nenhuma,
        execucoes: regras.iter().map(|r| execucao(r)).collect(),
    }
}

fn ambiguo(entrada: &str, regras: &[&str]) -> Leitura {
    Leitura {
        decisao: Decisao::Ambiguo {
            candidatas: Vec::new(),
            motivo:
                "regras empatadas em prioridade 0 e especificidade 1 discordam em tipo_documental"
                    .to_owned(),
        },
        ..casou(entrada, regras)
    }
}

impl Banco {
    fn novo() -> Self {
        let (inventario, comandos) = demonstracao::nova();
        // Os achados da tabela de A triar: o que a tabela genérica desenha.
        let tabela = inventario
            .tabela(&PedidoDeTabela::Area {
                area: Area::ATriar,
                filtro: Filtro::default(),
            })
            .unwrap();
        let achados: Vec<String> = tabela
            .linhas
            .iter()
            .filter_map(|l| l.alvo.as_ref().map(|a| a.chave.clone()))
            .take(3)
            .collect();
        assert!(achados.len() >= 2, "a demonstração tem achados a triar");

        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        let regras = FonteDasRegras::ficticia(None).unwrap();
        janela.regras.ligar(regras.clone());
        // A Caixa de entrada da demonstração, com o Pix e o ambíguo reconhecidos pelo motor
        // do catálogo fictício (que grava a execução e a decisão, como o real).
        let reconhecedor = Arc::new(reconhecedor_de_demonstracao(Some(&regras)));
        janela.ligar_caixa_de_entrada(Arc::new(CaixaDeEntradaFicticia::nova()));
        janela.ligar_reconhecedor(reconhecedor);
        let app = AppSbf::novo(
            janela,
            Box::new(inventario),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self {
            ctx,
            casca,
            regras,
            achados,
        };
        b.quadro();
        b
    }

    fn quadro(&mut self) {
        self.quadro_com_formas();
    }

    /// Desenha um quadro e devolve o que ele pintou.
    fn quadro_com_formas(&mut self) -> Vec<egui::epaint::ClippedShape> {
        let Banco { ctx, casca, .. } = self;
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 900.0),
            )),
            ..Default::default()
        };
        let mut saida = ctx.run_ui(entrada, |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
        saida.shapes
    }

    fn abrir(&mut self, destino: Destino) {
        bancada_casca::abrir(&self.ctx, destino.rota());
        for _ in 0..4 {
            self.quadro();
        }
        assert!(
            self.casca.estado.erros.is_empty(),
            "{:?}",
            self.casca.estado.erros
        );
    }

    fn cor(&mut self, sha: &str) -> CorDaMarca {
        self.casca.app.janela().marcacao.lock().unwrap().cor(sha)
    }

    fn consultas(&mut self) -> usize {
        self.casca.app.janela().marcacao.lock().unwrap().consultas
    }
}

#[test]
fn a_tabela_de_achados_pinta_verde_ambar_e_sem_cor_e_o_selo_conta_os_padroes() {
    let mut b = Banco::novo();
    let (verde, ambar, sem_cor) = (
        b.achados[0].clone(),
        b.achados[1].clone(),
        "nada".to_owned(),
    );
    b.regras
        .registrar_leitura(&casou(&verde, &["padrao-a", "padrao-b"]))
        .unwrap();
    b.regras
        .registrar_leitura(&ambiguo(&ambar, &["padrao-a", "padrao-b"]))
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    assert_eq!(b.cor(&verde), CorDaMarca::Verde);
    assert_eq!(b.cor(&ambar), CorDaMarca::Ambar);
    assert_eq!(b.cor(&sem_cor), CorDaMarca::SemCor);
    let m = b.casca.app.janela().marcacao.lock().unwrap().marca(&verde);
    assert_eq!(m.padroes, 2);
    assert_eq!(sbf_janela::marcacao::texto_do_selo(m), "2 padrões");
}

fn formas(f: &egui::Shape, fundos: &mut Vec<egui::Color32>, textos: &mut Vec<String>) {
    match f {
        egui::Shape::Rect(r) => fundos.push(r.fill),
        egui::Shape::Text(t) => textos.push(t.galley.text().to_owned()),
        egui::Shape::Vec(v) => v.iter().for_each(|x| formas(x, fundos, textos)),
        _ => {}
    }
}

fn pintado(b: &mut Banco) -> (Vec<egui::Color32>, Vec<String>) {
    let (mut fundos, mut textos) = (Vec::new(), Vec::new());
    for c in b.quadro_com_formas() {
        formas(&c.shape, &mut fundos, &mut textos);
    }
    (fundos, textos)
}

#[test]
fn as_linhas_saem_pintadas_com_o_selo_e_o_filtro_tira_as_outras_cores() {
    let mut b = Banco::novo();
    b.regras
        .registrar_leitura(&casou(&b.achados[0].clone(), &["padrao-a", "padrao-b"]))
        .unwrap();
    b.regras
        .registrar_leitura(&ambiguo(&b.achados[1].clone(), &["padrao-a"]))
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    let p = sbf_janela::tema::paleta(&b.ctx);
    let (fundos, textos) = pintado(&mut b);
    assert!(fundos.contains(&p.marca_verde), "linha verde");
    assert!(fundos.contains(&p.marca_ambar), "linha âmbar");
    assert!(textos.iter().any(|t| t == "2 padrões"), "{textos:?}");
    assert!(textos.iter().any(|t| t == "1 padrão"), "{textos:?}");
    // O filtro Verde deixa só as linhas verdes.
    let chave = format!("area:{:?}", Area::ATriar);
    b.casca
        .app
        .janela()
        .marcacao
        .lock()
        .unwrap()
        .definir_filtro(&chave, FiltroDeCor::Verde);
    for _ in 0..3 {
        b.quadro();
    }
    let (fundos, _) = pintado(&mut b);
    assert!(fundos.contains(&p.marca_verde));
    assert!(!fundos.contains(&p.marca_ambar), "o âmbar saiu da lista");
    // Sem cor: nenhuma linha colorida.
    b.casca
        .app
        .janela()
        .marcacao
        .lock()
        .unwrap()
        .definir_filtro(&chave, FiltroDeCor::SemCor);
    for _ in 0..3 {
        b.quadro();
    }
    let (fundos, _) = pintado(&mut b);
    assert!(!fundos.contains(&p.marca_verde) && !fundos.contains(&p.marca_ambar));
}

#[test]
fn uma_consulta_em_lote_para_as_linhas_visiveis_e_nenhuma_por_quadro() {
    let mut b = Banco::novo();
    b.regras
        .registrar_leitura(&casou(&b.achados[0].clone(), &["padrao-a"]))
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    let depois_de_abrir = b.consultas();
    assert!(depois_de_abrir >= 1);
    assert!(
        depois_de_abrir <= 4,
        "no máximo uma consulta em lote por lista, e não uma por linha ({depois_de_abrir})"
    );
    // Quadros seguintes dentro da validade não consultam de novo.
    for _ in 0..30 {
        b.quadro();
    }
    assert_eq!(b.consultas(), depois_de_abrir);
}

#[test]
fn o_filtro_separa_os_tres_grupos_e_so_a_cor_escolhida_passa() {
    let mut b = Banco::novo();
    let (verde, ambar) = (b.achados[0].clone(), b.achados[1].clone());
    b.regras
        .registrar_leitura(&casou(&verde, &["padrao-a"]))
        .unwrap();
    b.regras
        .registrar_leitura(&ambiguo(&ambar, &["padrao-a", "padrao-b"]))
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    let ids = [verde.as_str(), ambar.as_str(), "sem-padrao"];
    let j = b.casca.app.janela();
    let mut m = j.marcacao.lock().unwrap();
    for (filtro, esperado) in [
        (FiltroDeCor::Todas, [true, true, true]),
        (FiltroDeCor::Verde, [true, false, false]),
        (FiltroDeCor::Ambar, [false, true, false]),
        (FiltroDeCor::SemCor, [false, false, true]),
    ] {
        m.definir_filtro("lista", filtro);
        assert_eq!(m.passam("lista", &ids), esperado, "{filtro:?}");
    }
    drop(m);
}

#[test]
fn com_o_filtro_ligado_a_tabela_desenha_so_a_cor_escolhida_sem_erro() {
    let mut b = Banco::novo();
    let (verde, ambar) = (b.achados[0].clone(), b.achados[1].clone());
    b.regras
        .registrar_leitura(&casou(&verde, &["padrao-a"]))
        .unwrap();
    b.regras
        .registrar_leitura(&ambiguo(&ambar, &["padrao-a", "padrao-b"]))
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    let chave = format!("area:{:?}", Area::ATriar);
    for filtro in [FiltroDeCor::Verde, FiltroDeCor::Ambar, FiltroDeCor::SemCor] {
        b.casca
            .app
            .janela()
            .marcacao
            .lock()
            .unwrap()
            .definir_filtro(&chave, filtro);
        for _ in 0..4 {
            b.quadro();
        }
        assert!(
            b.casca.estado.erros.is_empty(),
            "{filtro:?}: {:?}",
            b.casca.estado.erros
        );
    }
    b.casca
        .app
        .janela()
        .marcacao
        .lock()
        .unwrap()
        .definir_filtro(&chave, FiltroDeCor::Todas);
    b.quadro();
}

#[test]
fn desativar_o_padrao_mantem_o_verde_nas_listas() {
    let mut b = Banco::novo();
    let achado = b.achados[0].clone();
    let antes = Leitura {
        decisao: Decisao::Nenhuma,
        ..casou(&achado, &["comprovante-pix-banco-exemplo"])
    };
    b.regras.registrar_leitura(&antes).unwrap();
    // Desativa a regra do Pix no catálogo.
    b.regras
        .catalogo
        .lock()
        .unwrap()
        .desativar("comprovante-pix-banco-exemplo", &sbf_regras::Quem::Usuario)
        .unwrap();
    b.abrir(Destino::Area(Area::ATriar));
    assert_eq!(b.cor(&achado), CorDaMarca::Verde);
}

#[test]
fn a_caixa_de_entrada_da_demonstracao_pinta_o_pix_de_verde_e_o_ambiguo_de_ambar() {
    let (inventario, comandos) = demonstracao::nova();
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let mut janela = Janela::nova();
    let regras = FonteDasRegras::ficticia(None).unwrap();
    janela.regras.ligar(regras.clone());
    sbf_janela::reconhecimento_ficticio::ligar_na_demonstracao(&mut janela);
    let app = AppSbf::novo(
        janela,
        Box::new(inventario),
        Box::new(comandos),
        EstadoDaBarra::default(),
    );
    let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
    let mut b = Banco {
        ctx,
        casca,
        regras,
        achados: Vec::new(),
    };
    b.quadro();
    b.abrir(Destino::AreaFutura("caixa"));
    let marcas = b
        .regras
        .marcas(&[
            sha(NOME_DO_PIX),
            sha(NOME_DO_AMBIGUO),
            sha(NOME_DO_RECIBO_SEM_PADRAO),
        ])
        .unwrap();
    assert_eq!(marcas[&sha(NOME_DO_PIX)].cor(), CorDaMarca::Verde);
    assert_eq!(marcas[&sha(NOME_DO_AMBIGUO)].cor(), CorDaMarca::Ambar);
    assert!(!marcas.contains_key(&sha(NOME_DO_RECIBO_SEM_PADRAO)));
    assert_eq!(b.cor(&sha(NOME_DO_PIX)), CorDaMarca::Verde);
    assert_eq!(b.cor(&sha(NOME_DO_AMBIGUO)), CorDaMarca::Ambar);
    assert_eq!(b.cor(&sha(NOME_DO_RECIBO_SEM_PADRAO)), CorDaMarca::SemCor);
    // A Caixa de entrada pinta o Pix de verde e o ambíguo de âmbar, com o selo.
    let p = sbf_janela::tema::paleta(&b.ctx);
    let (fundos, textos) = pintado(&mut b);
    assert!(fundos.contains(&p.marca_verde), "o Pix saiu verde");
    assert!(fundos.contains(&p.marca_ambar), "o ambíguo saiu âmbar");
    assert!(
        textos
            .iter()
            .any(|t| t.ends_with("padrão") || t.ends_with("padrões"))
    );
    // A seção "Sem padrão de reconhecimento" de A triar: o recibo que um padrão casou antes e
    // hoje não casa mais continua verde; o filtro Sem cor o esconde.
    let recibo = sha(NOME_DO_RECIBO_SEM_PADRAO);
    b.regras
        .registrar_leitura(&casou(&recibo, &["padrao-desativado"]))
        .unwrap();
    b.casca.app.janela().marcacao.lock().unwrap().esquecer();
    b.abrir(Destino::Area(Area::ATriar));
    for _ in 0..3 {
        b.quadro();
    }
    assert_eq!(b.cor(&recibo), CorDaMarca::Verde);
    let (fundos, textos) = pintado(&mut b);
    assert!(fundos.contains(&p.marca_verde));
    assert!(textos.iter().any(|t| t == "1 padrão"), "{textos:?}");
    b.casca
        .app
        .janela()
        .marcacao
        .lock()
        .unwrap()
        .definir_filtro("a-triar-sem-padrao-tabela", FiltroDeCor::SemCor);
    for _ in 0..3 {
        b.quadro();
    }
    let (_, textos) = pintado(&mut b);
    assert!(
        textos.iter().any(|t| t == "Nenhum documento desta cor."),
        "{textos:?}"
    );
    assert!(b.casca.estado.erros.is_empty());
}
