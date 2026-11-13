//! Testes sem janela da Prévia do arquivo (spec 06, "Prévia do arquivo"): a janela dentro da
//! `bancada-casca`, desenhada com `egui::Context::run_ui`, com os dados e os arquivos
//! fictícios da demonstração. O abridor é falso (o desenho real é do crate `sbf-previa`):
//! aqui se testa a janela, não o desenho do PDF.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bancada_casca::Casca;
use sbf_inventario::consulta::{Area, Filtro, PedidoDeTabela};
use sbf_inventario::{Consulta, Ref, Tipo};
use sbf_janela::demonstracao::{self, ConsultaDemonstracao};
use sbf_janela::pagina::{self, ConteudoDaAba};
use sbf_janela::previa::{self, Abridor, Posto, Previas, Resumo};
use sbf_janela::{AppSbf, EstadoDaBarra, Janela, Pedido};
use sbf_previa::{Anexo, Conteudo, Email, ErroDaPrevia, Imagem};

/// O que o abridor falso abriu, pelo nome do arquivo no acervo.
type Abertos = Arc<Mutex<Vec<String>>>;

fn imagem_ficticia() -> Imagem {
    Imagem {
        largura: 4,
        altura: 6,
        rgba: vec![200; 4 * 6 * 4],
    }
}

/// PDF e PNG viram uma imagem; e-mail vira um e-mail; o resto, o erro dado.
fn abridor_falso(abertos: Abertos, espera: Duration, erro: fn() -> ErroDaPrevia) -> Abridor {
    Arc::new(move |p: sbf_previa::Pedido<'_>| {
        let nome = p
            .arquivo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(mut a) = abertos.lock() {
            a.push(nome);
        }
        std::thread::sleep(espera);
        match p.extensao {
            "pdf" | "png" => Ok(Conteudo::Imagem(imagem_ficticia())),
            "eml" => Ok(Conteudo::Email(Email {
                de: "administracao@kappa.example".into(),
                para: "cliente@exemplo.example".into(),
                assunto: "Boleto do condomínio · dezembro".into(),
                data: "Wed, 11 Nov 2026 09:14:00 -0300".into(),
                corpo: "Segue o boleto.".into(),
                anexos: vec![Anexo {
                    nome: "boleto-kappa-dez.pdf".into(),
                    bytes: 2048,
                    sha256: None,
                }],
            })),
            _ => Err(erro()),
        }
    })
}

fn nao_implementado() -> ErroDaPrevia {
    ErroDaPrevia::NaoImplementado
}

struct Banco {
    ctx: egui::Context,
    casca: Casca<AppSbf>,
    consulta: ConsultaDemonstracao,
    tempo: f64,
    _pasta: tempfile::TempDir,
}

impl Banco {
    fn novo(abridor: Abridor) -> Self {
        let (consulta, comandos) = demonstracao::nova();
        let pasta = tempfile::tempdir().expect("pasta");
        let n = consulta
            .gerar_acervo(pasta.path())
            .expect("acervo fictício");
        assert!(n > 10, "a demonstração gera os arquivos fictícios: {n}");
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = "2026-11-13".into();
        janela.previas = Previas::com_abridor(abridor);
        janela.previas.definir_pasta(pasta.path().to_path_buf());
        let app = AppSbf::novo(
            janela,
            Box::new(consulta.clone()),
            Box::new(comandos),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut b = Self {
            ctx,
            casca,
            consulta,
            tempo: 0.0,
            _pasta: pasta,
        };
        b.quadro();
        b
    }

    fn entrada(&mut self, eventos: Vec<egui::Event>) -> egui::RawInput {
        self.tempo += 1.0 / 60.0;
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            time: Some(self.tempo),
            events: eventos,
            ..Default::default()
        }
    }

    /// Um quadro; devolve todo o texto desenhado.
    fn quadro_com(&mut self, eventos: Vec<egui::Event>) -> String {
        let entrada = self.entrada(eventos);
        let Banco { ctx, casca, .. } = self;
        let mut saida = ctx.run_ui(entrada, |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
        textos(&saida.shapes)
    }

    fn quadro(&mut self) -> String {
        self.quadro_com(Vec::new())
    }

    fn abrir(&mut self, area: Area) {
        bancada_casca::abrir(&self.ctx, sbf_janela::Destino::Area(area).rota());
        self.quadro();
        self.quadro();
    }

    /// Abre a página do item no centro (a aba Arquivo é a primeira) e devolve o texto.
    fn abrir_item(&mut self, r: &Ref) -> String {
        bancada_casca::abrir(&self.ctx, sbf_janela::Destino::Item(r.clone()).rota());
        self.quadro();
        self.quadro();
        self.quadro()
    }

    fn focar(&mut self, r: &Ref) {
        self.casca.app.janela().centro.selecao.foco = Some(r.clone());
    }

    /// Quadros até a condição valer (ou o prazo passar); devolve o texto do último.
    fn ate(&mut self, prazo: Duration, mut cond: impl FnMut(&Self) -> bool) -> String {
        let inicio = Instant::now();
        let mut texto = self.quadro();
        while !cond(self) && inicio.elapsed() < prazo {
            std::thread::sleep(Duration::from_millis(10));
            texto = self.quadro();
        }
        texto
    }

    fn clicar(&mut self, ponto: egui::Pos2) {
        self.quadro_com(vec![egui::Event::PointerMoved(ponto)]);
        self.quadro_com(vec![egui::Event::PointerButton {
            pos: ponto,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.quadro_com(vec![egui::Event::PointerButton {
            pos: ponto,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }

    /// Os achados de uma área, na ordem da tabela.
    fn da_area(&self, area: Area) -> Vec<Ref> {
        self.consulta
            .tabela(&PedidoDeTabela::Area {
                area,
                filtro: Filtro::default(),
            })
            .expect("tabela")
            .linhas
            .iter()
            .filter_map(|l| l.alvo.clone())
            .collect()
    }

    fn achado_chamado(&self, nome: &str) -> Ref {
        for area in [Area::ATriar, Area::Documentos, Area::EmDuvida] {
            for r in self.da_area(area) {
                if self
                    .consulta
                    .item(&r)
                    .ok()
                    .flatten()
                    .is_some_and(|i| i.rotulo == nome)
                {
                    return r;
                }
            }
        }
        panic!("a demonstração não tem {nome}");
    }
}

fn textos(formas: &[egui::epaint::ClippedShape]) -> String {
    fn de(forma: &egui::Shape, saida: &mut String) {
        match forma {
            egui::Shape::Text(t) => {
                saida.push_str(t.galley.text());
                saida.push('\n');
            }
            egui::Shape::Vec(v) => v.iter().for_each(|f| de(f, saida)),
            _ => {}
        }
    }
    let mut s = String::new();
    for f in formas {
        de(&f.shape, &mut s);
    }
    s
}

fn abertos() -> Abertos {
    Arc::new(Mutex::new(Vec::new()))
}

#[test]
fn o_painel_previa_e_visivel_por_padrao_embaixo_do_detalhe() {
    use bancada_casca::AppCasca;
    let b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    let paineis = b.casca.app.paineis();
    let previa = paineis
        .iter()
        .find(|p| p.id == "previa")
        .expect("painel Prévia");
    let detalhe = paineis.iter().find(|p| p.id == "detalhe").expect("Detalhe");
    let historico = paineis
        .iter()
        .find(|p| p.id == "historico")
        .expect("Histórico");
    assert!(previa.visivel_por_padrao);
    assert!(b.casca.painel_visivel("previa"));
    assert!(detalhe.pilha < previa.pilha && previa.pilha < historico.pilha);
}

#[test]
fn a_aba_arquivo_e_a_primeira_em_achado_e_documento() {
    for tipo in [Tipo::Achado, Tipo::Documento] {
        assert_eq!(
            pagina::abas_de(tipo)[0].conteudo,
            ConteudoDaAba::Arquivo,
            "{tipo:?}"
        );
        assert!(pagina::abas_de(tipo)[0].motivo.is_none());
    }
    for tipo in [Tipo::Origem, Tipo::Registro] {
        assert!(
            pagina::abas_de(tipo)
                .iter()
                .any(|a| a.conteudo == ConteudoDaAba::Arquivo),
            "{tipo:?}"
        );
    }
}

#[test]
fn o_painel_mostra_a_miniatura_do_documento_em_foco() {
    let mut b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    b.abrir(Area::Documentos);
    let doc = b.da_area(Area::Documentos)[0].clone();
    b.focar(&doc);
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.tem_imagem(&doc.chave, 1)
    });
    assert!(b.casca.app.janela().previas.tem_imagem(&doc.chave, 1));
    assert_eq!(
        b.casca.app.janela().previas.resumo(&doc.chave),
        Some(&Resumo::Imagem)
    );
    // As texturas só das prévias desenhadas: o painel mostra uma.
    assert!(b.casca.app.janela().previas.texturas_vivas() <= 2);
}

#[test]
fn a_origem_e_o_registro_mostram_o_documento_deles() {
    let mut b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    let doc = b.da_area(Area::Documentos)[0].clone();
    let registro = b
        .consulta
        .tabela(&PedidoDeTabela::Ligados {
            de: doc.clone(),
            tipo: Tipo::Registro,
        })
        .expect("registros")
        .linhas
        .iter()
        .find_map(|l| l.alvo.clone())
        .expect("um registro");
    b.focar(&registro);
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.resumo(&doc.chave).is_some()
    });
    assert_eq!(
        b.casca.app.janela().previas.resumo(&doc.chave),
        Some(&Resumo::Imagem)
    );
}

#[test]
fn destacar_pede_a_janela_propria() {
    let mut b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    b.abrir(Area::Documentos);
    let doc = b.da_area(Area::Documentos)[0].clone();
    b.focar(&doc);
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.tem_imagem(&doc.chave, 1)
    });
    let botao = b
        .ctx
        .read_response(egui::Id::new(("previa-destacar", Posto::Painel)))
        .expect("o botão Destacar do painel");
    b.clicar(botao.rect.center());
    let destacadas: Vec<String> = b
        .casca
        .app
        .janela()
        .previas
        .destacadas()
        .iter()
        .map(|d| d.achado.chave.clone())
        .collect();
    assert_eq!(destacadas, vec![doc.chave.clone()]);
    // Sem janelas múltiplas (teste), a janela própria aparece embutida, com o título.
    let nome = b.consulta.item(&doc).expect("item").expect("doc").rotulo;
    let texto = b.quadro();
    assert!(texto.contains(&format!("Prévia · {nome}")), "{texto}");
}

#[test]
fn a_dica_com_a_miniatura_so_aparece_depois_de_meio_segundo() {
    let (consulta, _) = demonstracao::nova();
    let pasta = tempfile::tempdir().expect("pasta");
    consulta.gerar_acervo(pasta.path()).expect("acervo");
    let ctx = egui::Context::default();
    sbf_janela::tema::instalar_fontes(&ctx);
    let mut previas =
        Previas::com_abridor(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    previas.definir_pasta(pasta.path().to_path_buf());
    let mut cache = sbf_janela::leitura::Cache::default();
    let doc = consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::Documentos,
            filtro: Filtro::default(),
        })
        .expect("tabela")
        .linhas[0]
        .alvo
        .clone()
        .expect("documento");
    let mut quadro = |tempo: f64, mouse: egui::Pos2| -> Option<Ref> {
        let entrada = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            time: Some(tempo),
            events: vec![egui::Event::PointerMoved(mouse)],
            ..Default::default()
        };
        let mut pedidos: Vec<Pedido> = Vec::new();
        let mut saida = ctx.run_ui(entrada, |ui| {
            sbf_janela::componentes::link_item(ui, &doc, "conta-energia-out.pdf", &mut pedidos);
            let mut leitor = sbf_janela::leitura::Leitor::novo(&consulta, &mut cache);
            previas.janelas_ui(ui.ctx(), &mut leitor, &mut pedidos);
        });
        saida.textures_delta.clear();
        previas.dica_visivel().cloned()
    };
    let sobre = egui::pos2(12.0, 10.0);
    // O primeiro quadro só posiciona o link (o egui acha o hover pelo quadro anterior).
    assert_eq!(quadro(0.0, sobre), None);
    // O mouse parado sobre o nome desde 0,01 s.
    assert_eq!(quadro(0.01, sobre), None);
    assert_eq!(quadro(0.3, sobre), None);
    assert_eq!(quadro(0.5, sobre), None);
    assert_eq!(quadro(0.52, sobre), Some(doc.clone()));
    assert_eq!(quadro(0.6, sobre), Some(doc.clone()));
    // O mouse sai: a dica some, e voltar recomeça a contar.
    assert_eq!(quadro(0.7, egui::pos2(500.0, 500.0)), None);
    assert_eq!(quadro(0.8, sobre), None);
    assert_eq!(quadro(1.2, sobre), None);
    assert_eq!(quadro(1.31, sobre), Some(doc));
}

#[test]
fn mudar_a_selecao_depressa_so_desenha_o_ultimo() {
    let lista = abertos();
    let mut b = Banco::novo(abridor_falso(
        Arc::clone(&lista),
        Duration::from_millis(300),
        nao_implementado,
    ));
    b.abrir(Area::Documentos);
    let docs: Vec<Ref> = b.da_area(Area::Documentos).into_iter().take(5).collect();
    assert_eq!(docs.len(), 5);
    // O primeiro começa a abrir…
    b.focar(&docs[0]);
    b.quadro();
    let inicio = Instant::now();
    while lista.lock().expect("lista").is_empty() && inicio.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
    }
    // …e a seleção passa por três antes de parar no último.
    for d in &docs[1..] {
        b.focar(d);
        b.quadro();
    }
    let ultimo = docs[4].chave.clone();
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.tem_imagem(&ultimo, 1)
    });
    assert!(b.casca.app.janela().previas.tem_imagem(&ultimo, 1));
    let abertos = b.casca.app.janela().previas.abertos.clone();
    assert_eq!(
        abertos,
        vec![docs[0].chave.clone(), ultimo],
        "só o primeiro (já começado) e o último"
    );
    for d in &docs[1..4] {
        assert!(b.casca.app.janela().previas.resumo(&d.chave).is_none());
    }
}

#[test]
fn erro_da_previa_mostra_os_metadados_e_o_motivo_sem_panico() {
    let mut b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    b.abrir(Area::ATriar);
    // O XML da nota: o abridor falso devolve NaoImplementado.
    let nota = b.achado_chamado("nfe-35261111.xml");
    b.focar(&nota);
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.resumo(&nota.chave).is_some()
    });
    let motivo = previa::motivo_do_erro(&ErroDaPrevia::NaoImplementado);
    // No painel: o motivo.
    let texto = b.quadro();
    assert!(matches!(
        b.casca.app.janela().previas.resumo(&nota.chave),
        Some(Resumo::Erro { .. })
    ));
    assert!(
        texto.contains("Sem prévia") && texto.contains(&motivo),
        "{texto}"
    );
    // Na aba Arquivo (a primeira da página): os metadados inteiros.
    let texto = b.abrir_item(&nota);
    for esperado in [
        "Sem prévia",
        motivo.as_str(),
        "nfe-35261111.xml",
        "Tamanho",
        "Data-chave",
        "12/11/2026",
        "Abrir no programa do sistema",
    ] {
        assert!(texto.contains(esperado), "falta {esperado:?} em:\n{texto}");
    }
}

#[test]
fn sem_previa_e_sem_copia_no_acervo_mostram_o_motivo() {
    let sem_previa: Abridor = Arc::new(|_p: sbf_previa::Pedido<'_>| {
        Ok(Conteudo::SemPrevia {
            motivo: "Sem prévia para este tipo de arquivo.".into(),
        })
    });
    let mut b = Banco::novo(sem_previa);
    b.abrir(Area::ATriar);
    let csv = b.achado_chamado("extrato-alfa-2026-10.csv");
    b.focar(&csv);
    b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.resumo(&csv.chave).is_some()
    });
    let texto = b.quadro();
    assert!(
        texto.contains("Sem prévia para este tipo de arquivo."),
        "{texto}"
    );
    assert!(texto.contains("extrato-alfa-2026-10.csv"));
    // A foto do WhatsApp não tem cópia no acervo da demonstração.
    let foto = b.achado_chamado("WhatsApp Image 2026-11-12.jpg");
    b.focar(&foto);
    let texto = b.quadro();
    assert!(
        texto.contains("Este achado ainda não tem cópia no acervo."),
        "{texto}"
    );
}

#[test]
fn protegido_sem_regra_mostra_nova_regra() {
    let lista = abertos();
    let mut b = Banco::novo(abridor_falso(
        Arc::clone(&lista),
        Duration::ZERO,
        nao_implementado,
    ));
    b.abrir(Area::ATriar);
    let protegido = b.achado_chamado("fatura-2026-11-recebida.pdf");
    b.focar(&protegido);
    b.ate(Duration::from_secs(5), |b| {
        b.casca
            .app
            .janela()
            .previas
            .resumo(&protegido.chave)
            .is_some()
    });
    assert_eq!(
        b.casca.app.janela().previas.resumo(&protegido.chave),
        Some(&Resumo::Protegido {
            motivo: previa::MOTIVO_SEM_REGRA.to_owned()
        })
    );
    let texto = b.quadro();
    assert!(texto.contains("Protegido por senha"), "{texto}");
    let texto = b.abrir_item(&protegido);
    assert!(texto.contains("Protegido por senha"), "{texto}");
    assert!(texto.contains("Nova regra"), "{texto}");
    // Sem candidata nem se tenta abrir.
    assert!(lista.lock().expect("lista").is_empty());
    // Nova regra abre o diálogo.
    b.casca.app.janela().pedir(Pedido::NovaRegra {
        fornecedor: "Banco Beta".into(),
        tipo_de_documento: "fatura_cartao".into(),
    });
    b.quadro();
    assert!(matches!(
        b.casca.app.janela().dialogo,
        Some(sbf_janela::janela::Dialogo::NovaRegra { ref fornecedor, .. }) if fornecedor == "Banco Beta"
    ));
}

#[test]
fn a_aba_arquivo_mostra_o_email_com_o_anexo() {
    let mut b = Banco::novo(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    let eml = b.achado_chamado("cobranca-condominio-kappa.eml");
    bancada_casca::abrir(&b.ctx, sbf_janela::Destino::Item(eml.clone()).rota());
    b.quadro();
    let texto = b.ate(Duration::from_secs(5), |b| {
        b.casca.app.janela().previas.resumo(&eml.chave).is_some()
    });
    let texto = if texto.contains("Anexos") {
        texto
    } else {
        b.quadro()
    };
    for esperado in [
        "Arquivo",
        "Boleto do condomínio · dezembro",
        "administracao@kappa.example",
        "Anexos (1)",
        "boleto-kappa-dez.pdf",
        "Caber na largura",
    ] {
        assert!(texto.contains(esperado), "falta {esperado:?} em:\n{texto}");
    }
}

#[test]
fn a_demonstracao_gera_o_anexo_com_o_mesmo_hash_do_achado() {
    let (consulta, _) = demonstracao::nova();
    let pasta = tempfile::tempdir().expect("pasta");
    consulta.gerar_acervo(pasta.path()).expect("acervo");
    let anexo = consulta
        .tabela(&PedidoDeTabela::Area {
            area: Area::ATriar,
            filtro: Filtro::default(),
        })
        .expect("tabela")
        .linhas
        .iter()
        .filter_map(|l| l.alvo.clone())
        .find(|r| {
            consulta
                .item(r)
                .ok()
                .flatten()
                .is_some_and(|i| i.rotulo == "boleto-kappa-dez.pdf")
        })
        .expect("o anexo é um achado");
    let item = consulta.item(&anexo).expect("item").expect("achado");
    let rel = item
        .campos
        .iter()
        .find(|c| c.rotulo == "No cofre")
        .map(|c| sbf_janela::formato::texto_da_celula(&c.valor))
        .expect("no cofre");
    let bytes = std::fs::read(pasta.path().join(rel)).expect("arquivo");
    assert_eq!(sbf_janela::acervo_ficticio::sha256_hex(&bytes), anexo.chave);
    // E o .eml traz o mesmo PDF em base64.
    let eml_rel = std::fs::read_dir(pasta.path().join("acervo"))
        .expect("acervo")
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "eml"))
        .expect("um .eml");
    let eml = std::fs::read_to_string(eml_rel).expect("eml");
    let base64 = sbf_janela::acervo_ficticio::base64(&bytes);
    assert!(eml.contains(base64.lines().next().expect("linha")));
}

#[test]
fn opcoes_mudam_os_limites_do_cache() {
    let mut p = Previas::com_abridor(abridor_falso(abertos(), Duration::ZERO, nao_implementado));
    assert_eq!(p.limites(), (30, 256 * 1024 * 1024));
    p.definir_limites(10, 64 * 1024 * 1024);
    assert_eq!(p.limites(), (10, 64 * 1024 * 1024));
    // A pasta temporária da demonstração é apagada ao sair.
    let pasta = tempfile::tempdir().expect("pasta").keep();
    std::fs::write(pasta.join("x.txt"), b"x").expect("arquivo");
    p.definir_pasta_temporaria(pasta.clone());
    drop(p);
    assert!(!pasta.exists());
}
