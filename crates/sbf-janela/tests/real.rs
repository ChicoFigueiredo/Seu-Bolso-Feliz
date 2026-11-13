//! A janela sobre o inventário **real** (`LeitorSqlite` + `Inventario`), com a fixture
//! fictícia do `sbf-inventario` (INV-17): toda área, a página de cada tipo que aparece nos
//! links, todas as abas, uma decisão pela janela e o desfazer. E o tempo de desenho com
//! 50 mil achados (INV-16, parcial: sem janela, sem GPU).

#[path = "../../sbf-inventario/tests/comum/mod.rs"]
mod comum;

use std::time::{Duration, Instant};

use bancada_casca::Casca;
use sbf_inventario::banco::Inventario;
use sbf_inventario::consulta::{Area, Celula, Filtro, PedidoDeTabela};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::{Consulta, Ref};
use sbf_janela::{AppSbf, Destino, EstadoDaBarra, IdAcao, Janela};

fn entrada() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    }
}

struct Real {
    ctx: egui::Context,
    casca: Casca<AppSbf>,
}

impl Real {
    fn novo(caminho: &std::path::Path) -> Self {
        let inventario = Inventario::abrir(caminho).expect("abrir");
        assert!(!inventario.plano().expect("plano").precisa_migrar());
        let leitor = LeitorSqlite::abrir(caminho).expect("leitor");
        let ctx = egui::Context::default();
        sbf_janela::tema::instalar_fontes(&ctx);
        let mut janela = Janela::nova();
        janela.hoje = "2026-11-13".into();
        let app = AppSbf::novo(
            janela,
            Box::new(leitor),
            Box::new(inventario),
            EstadoDaBarra::default(),
        );
        let casca = Casca::nova(app, sbf_janela::config_da_casca()).expect("temas");
        let mut r = Self { ctx, casca };
        r.quadro();
        r
    }

    fn quadro(&mut self) -> Duration {
        let inicio = Instant::now();
        let Real { ctx, casca } = self;
        let mut saida = ctx.run_ui(entrada(), |ui| casca.desenhar(ui));
        saida.textures_delta.clear();
        inicio.elapsed()
    }

    fn abrir(&mut self, d: Destino) {
        bancada_casca::abrir(&self.ctx, d.rota());
        self.quadro();
        self.quadro();
    }

    fn leitor(&self) -> &dyn Consulta {
        self.casca.app.consulta()
    }

    fn executar(&mut self, id: IdAcao) {
        bancada_casca::executar(&self.ctx, sbf_janela::acoes::definicao(id).codigo);
        self.quadro();
    }
}

fn alvos_das_areas(leitor: &dyn Consulta) -> Vec<Ref> {
    let mut v: Vec<Ref> = Vec::new();
    for area in sbf_janela::areas::AREAS_DO_INVENTARIO {
        let Ok(t) = leitor.tabela(&PedidoDeTabela::Area {
            area,
            filtro: Filtro::default(),
        }) else {
            continue;
        };
        for l in &t.linhas {
            for c in &l.celulas {
                let mut add = |r: &Ref| {
                    if !v.iter().any(|x| x.tipo == r.tipo) {
                        v.push(r.clone());
                    }
                };
                match c {
                    Celula::Link { alvo, .. } => add(alvo),
                    Celula::Links(l) => l.iter().for_each(|(r, _)| add(r)),
                    _ => {}
                }
            }
        }
    }
    v
}

#[test]
fn a_janela_desenha_o_inventario_real_inteiro() {
    let banco = comum::banco_de_teste();
    let mut r = Real::novo(&banco.caminho);
    for area in sbf_janela::areas::areas_da_fatia() {
        r.abrir(Destino::Area(area));
    }
    let alvos = alvos_das_areas(r.leitor());
    assert!(alvos.len() >= 8, "poucos tipos ligados: {alvos:?}");
    for alvo in alvos {
        r.abrir(Destino::Item(alvo.clone()));
        for i in 0..sbf_janela::pagina::abas_de(alvo.tipo).len() {
            r.casca.app.janela().centro.abas.insert(alvo.clone(), i);
            r.quadro();
        }
    }
}

#[test]
fn uma_decisao_pela_janela_grava_no_banco_e_o_desfazer_restaura() {
    let banco = comum::banco_de_teste();
    let mut r = Real::novo(&banco.caminho);
    r.abrir(Destino::Area(Area::ATriar));
    let antes = r.leitor().contagens().expect("contagens");
    let t = r
        .leitor()
        .tabela(&PedidoDeTabela::Area {
            area: Area::ATriar,
            filtro: Filtro::default(),
        })
        .expect("a triar");
    let Some(alvo) = t.linhas.iter().find_map(|l| l.alvo.clone()) else {
        panic!("a fixture não tem achado a triar");
    };
    r.casca.app.janela().centro.selecao.so(alvo);
    r.executar(IdAcao::Documento);
    let mensagem = r.casca.app.janela().mensagem.clone();
    assert!(mensagem.as_ref().is_some_and(|m| !m.erro), "{mensagem:?}");
    let depois = r.leitor().contagens().expect("contagens");
    assert_eq!(depois.a_triar, antes.a_triar - 1);
    r.executar(IdAcao::Desfazer);
    assert_eq!(r.leitor().contagens().expect("contagens"), antes);
}

#[test]
fn confirmar_conciliacoes_em_lote_grava_uma_decisao_por_par() {
    let banco = comum::banco_de_teste();
    {
        let conn = rusqlite::Connection::open(&banco.caminho).expect("abrir");
        let extrato = comum::Reg {
            fonte: "conexao:pluggy",
            tipo: "debito",
            data: "2026-10-20",
            valor: -77701,
            descricao: "Compra no extrato",
            conta: Some("Banco Alfa"),
            ..comum::Reg::default()
        };
        comum::inserir_registro(&conn, &extrato);
        let documento = comum::Reg {
            hash: Some(comum::hash(comum::H_MERCADO)),
            fonte: "gmail",
            tipo: "compra",
            data: "2026-10-21",
            valor: -77701,
            descricao: "Compra no documento",
            conta: Some("Banco Alfa"),
            ..comum::Reg::default()
        };
        comum::inserir_registro(&conn, &documento);
    }
    let contar = |sql: &str| -> i64 {
        rusqlite::Connection::open(&banco.caminho)
            .expect("abrir")
            .query_row(sql, [], |l| l.get(0))
            .expect("contar")
    };
    let conciliacoes =
        || contar("SELECT count(*) FROM relacao_registro WHERE tipo = 'conciliacao'");
    let decisoes = || contar("SELECT count(*) FROM decisao WHERE autor = 'usuario'");
    let (relacoes_antes, decisoes_antes) = (conciliacoes(), decisoes());

    let mut r = Real::novo(&banco.caminho);
    r.abrir(Destino::Area(Area::SugestoesDeConciliacao));
    let pedido = PedidoDeTabela::Area {
        area: Area::SugestoesDeConciliacao,
        filtro: Filtro::default(),
    };
    let t = r.leitor().tabela(&pedido).expect("sugestões");
    assert_eq!(t.linhas.len(), 2, "o par da fixture e o novo");
    // Sem seleção, a Ação não grava nada.
    r.executar(IdAcao::ConfirmarConciliacoes);
    assert_eq!(decisoes(), decisoes_antes);
    // Seleção múltipla: os dois pares.
    for l in &t.linhas {
        let alvo = l.alvo.clone().expect("alvo");
        r.casca.app.janela().centro.selecao.itens.push(alvo);
    }
    r.executar(IdAcao::ConfirmarConciliacoes);
    let mensagem = r.casca.app.janela().mensagem.clone();
    assert!(mensagem.as_ref().is_some_and(|m| !m.erro), "{mensagem:?}");
    assert_eq!(conciliacoes(), relacoes_antes + 2);
    assert_eq!(decisoes(), decisoes_antes + 2, "uma decisão por par");
    assert!(
        r.leitor()
            .tabela(&pedido)
            .expect("sugestões")
            .linhas
            .is_empty()
    );
    // Desfazer volta um par por vez.
    r.executar(IdAcao::Desfazer);
    assert_eq!(conciliacoes(), relacoes_antes + 1);
    assert_eq!(
        r.leitor().tabela(&pedido).expect("sugestões").linhas.len(),
        1
    );
}

#[test]
fn com_50_mil_achados_a_area_desenha_rapido() {
    let banco = comum::banco_de_teste();
    {
        let conn = rusqlite::Connection::open(&banco.caminho).expect("abrir");
        comum::popular_grande(&conn, 50_000);
    }
    let mut r = Real::novo(&banco.caminho);
    bancada_casca::abrir(&r.ctx, Destino::Area(Area::Documentos).rota());
    let primeiro = r.quadro() + r.quadro();
    let mut pior = Duration::ZERO;
    for _ in 0..5 {
        pior = pior.max(r.quadro());
    }
    eprintln!("primeiro quadro (com a consulta): {primeiro:?}; quadro seguinte (cache): {pior:?}");
    assert!(
        pior < Duration::from_millis(150),
        "quadro com cache lento: {pior:?}"
    );
    assert!(
        primeiro < Duration::from_secs(3),
        "abrir a área lento: {primeiro:?}"
    );
}

/// Gera um banco fictício para abrir com `sbf --inventario` e conferir a janela à mão:
/// `SBF_GERAR_BANCO=/tmp/x cargo test -p sbf-janela --test real -- --ignored gerar`.
#[test]
#[ignore = "só gera o banco de exemplo quando pedido"]
fn gerar_banco_de_exemplo() {
    let Some(pasta) = std::env::var_os("SBF_GERAR_BANCO") else {
        return;
    };
    let banco = comum::banco_de_teste();
    let destino = std::path::Path::new(&pasta).join("inventario.sqlite");
    std::fs::create_dir_all(&pasta).expect("pasta");
    std::fs::copy(&banco.caminho, &destino).expect("copiar");
}

#[test]
fn ferramentas_ligadas_ao_banco_real() {
    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;
    use sbf_janela::ferramentas::{Ferramentas, MOTIVO_SEM_DESTINO};

    let banco = comum::banco_de_teste();
    let cofre = tempfile::tempdir().expect("cofre");
    let pasta_do_chaveiro = tempfile::tempdir().expect("chaveiro");
    let mut r = Real::novo(&banco.caminho);
    let mut f = Ferramentas::nova(banco.caminho.clone(), cofre.path().to_path_buf());
    f.chaveiro = Some(
        sbf_chaveiro::Chaveiro::abrir_com_chaveiro(
            pasta_do_chaveiro.path(),
            std::sync::Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .expect("chaveiro"),
    );
    r.casca.app.janela().ferramentas = Some(f);

    // Sem destino, estante e cópia ficam desabilitadas com o motivo.
    let acoes = bancada_casca::AppCasca::acoes(&r.casca.app);
    for id in ["ferramentas.estante", "ferramentas.copia"] {
        let a = acoes.iter().find(|a| a.id == id).expect("ação");
        assert_eq!(a.motivo(), Some(MOTIVO_SEM_DESTINO));
    }

    // Regras de senha: só leitura, sem senha nenhuma.
    r.executar(IdAcao::RegrasDeSenha);
    let resumo = r
        .casca
        .app
        .janela()
        .ferramentas
        .as_ref()
        .and_then(|f| f.resumo());
    assert!(
        resumo.as_deref().is_some_and(|t| t.starts_with("regras:")),
        "{resumo:?}"
    );

    // PDF protegido: a rodada roda numa thread e o resultado volta à janela.
    r.executar(IdAcao::AbrirPdfProtegido);
    let ctx = r.ctx.clone();
    let terminou = r
        .casca
        .app
        .janela()
        .ferramentas
        .as_mut()
        .is_some_and(|f| f.esperar(&ctx, Duration::from_secs(60)));
    assert!(terminou, "a rodada não terminou");
    r.quadro();
    let resumo = r
        .casca
        .app
        .janela()
        .ferramentas
        .as_ref()
        .and_then(|f| f.resumo());
    assert!(
        resumo
            .as_deref()
            .is_some_and(|t| t.starts_with("pdf:") || t.starts_with("erro:")),
        "{resumo:?}"
    );

    // Cópia de segurança: sem senha no chaveiro, gera uma, guarda e mostra uma vez.
    r.casca
        .app
        .janela()
        .ferramentas
        .as_mut()
        .expect("ferramentas")
        .destino_da_copia = "destino-de-teste:SBF/Copia".into();
    r.casca
        .app
        .janela()
        .ferramentas
        .as_mut()
        .expect("ferramentas")
        .pedir_copia();
    {
        let j = r.casca.app.janela();
        let f = j.ferramentas.as_ref().expect("ferramentas");
        assert_eq!(
            f.resumo().as_deref(),
            Some("cópia: senha nova mostrada uma vez")
        );
        let rotulos = f.chaveiro.as_ref().expect("chaveiro").rotulos_de_senha();
        assert!(
            rotulos.iter().any(|r| r == "copia-de-seguranca"),
            "{rotulos:?}"
        );
    }
    // Com a senha no chaveiro, a próxima cópia já começa (aqui o rclone falha: erro claro).
    r.casca
        .app
        .janela()
        .ferramentas
        .as_mut()
        .expect("ferramentas")
        .pedir_copia();
    let terminou = r
        .casca
        .app
        .janela()
        .ferramentas
        .as_mut()
        .is_some_and(|f| f.esperar(&ctx, Duration::from_secs(60)));
    assert!(terminou);
    let resumo = r
        .casca
        .app
        .janela()
        .ferramentas
        .as_ref()
        .and_then(|f| f.resumo());
    assert!(
        resumo
            .as_deref()
            .is_some_and(|t| t.starts_with("erro:") || t.starts_with("cópia:")),
        "{resumo:?}"
    );

    // Abrir original de achado sem cópia no acervo dá um erro claro, sem pânico.
    let achado = sbf_inventario::Ref::nova(sbf_inventario::Tipo::Achado, "nao-existe");
    r.casca.app.janela().centro.selecao.so(achado);
    r.executar(IdAcao::AbrirOriginal);
    assert!(
        r.casca
            .app
            .janela()
            .mensagem
            .as_ref()
            .is_some_and(|m| m.erro)
    );
}

#[test]
fn areas_grandes_carregam_em_pedacos_e_ordenar_carrega_tudo() {
    let banco = comum::banco_de_teste();
    {
        let conn = rusqlite::Connection::open(&banco.caminho).expect("abrir");
        comum::popular_grande(&conn, 5_000);
    }
    let mut r = Real::novo(&banco.caminho);
    r.abrir(Destino::Area(Area::Documentos));
    let pedido = PedidoDeTabela::Area {
        area: Area::Documentos,
        filtro: Filtro::default(),
    };
    // Só o primeiro pedaço foi lido; a rolagem tem o tamanho do total.
    let c = r
        .casca
        .app
        .janela()
        .primeiras(r.casca.app.consulta(), &pedido, 0)
        .expect("pedaço");
    assert_eq!(c.tabela.linhas.len(), sbf_janela::leitura::PEDACO);
    assert!(c.total > sbf_janela::leitura::PEDACO, "total {}", c.total);
    // Ordenar com a tabela pela metade: aviso num quadro, tudo carregado e ordenado no seguinte.
    let chave = format!("area:{:?}", Area::Documentos);
    r.casca
        .app
        .janela()
        .centro
        .tabelas
        .entry(chave.clone())
        .or_default()
        .ordem_pendente = Some(Some((0, true)));
    r.quadro();
    r.quadro();
    let estado = r
        .casca
        .app
        .janela()
        .centro
        .tabelas
        .get(&chave)
        .cloned()
        .expect("estado");
    assert_eq!(estado.ordem, Some((0, true)));
    assert!(estado.ordem_pendente.is_none());
    let c = r
        .casca
        .app
        .janela()
        .primeiras(r.casca.app.consulta(), &pedido, 0)
        .expect("tudo");
    assert_eq!(c.tabela.linhas.len(), c.total);
}

#[test]
fn no_inventario_real_as_areas_e_a_visao_geral_carregam_em_segundo_plano() {
    let banco = comum::banco_de_teste();
    let cofre = tempfile::tempdir().expect("cofre");
    let mut r = Real::novo(&banco.caminho);
    {
        let mut j = r.casca.app.janela();
        j.ferramentas = Some(sbf_janela::ferramentas::Ferramentas::nova(
            banco.caminho.clone(),
            cofre.path().to_path_buf(),
        ));
        j.pre_carregar_visao_geral(banco.caminho.clone());
    }
    // A área pede o primeiro pedaço numa thread: o quadro não espera.
    bancada_casca::abrir(&r.ctx, Destino::Area(Area::ArquivosEPastas).rota());
    let pedido = PedidoDeTabela::Area {
        area: Area::PastasPessoais,
        filtro: Filtro::default(),
    };
    let inicio = Instant::now();
    loop {
        r.quadro();
        let pronto = r
            .casca
            .app
            .janela()
            .primeiras(r.casca.app.consulta(), &pedido, 0)
            .is_ok();
        if pronto && inicio.elapsed() > Duration::from_millis(50) {
            break;
        }
        assert!(inicio.elapsed() < Duration::from_secs(20), "não carregou");
        std::thread::sleep(Duration::from_millis(10));
    }
    // A Visão geral chega da thread e é desenhada.
    bancada_casca::abrir(&r.ctx, Destino::Area(Area::VisaoGeral).rota());
    for _ in 0..50 {
        r.quadro();
        std::thread::sleep(Duration::from_millis(10));
    }
}
