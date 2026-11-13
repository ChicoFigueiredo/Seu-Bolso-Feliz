//! A janela sobre o inventário **real** (`LeitorSqlite` + `Inventario`), com a fixture
//! fictícia do `sbf-inventario` (INV-17): toda área, a página de cada tipo que aparece nos
//! links, todas as abas, uma decisão pela janela e o desfazer. E o tempo de desenho com
//! 50 mil achados (INV-16, parcial: sem janela, sem GPU).

#[path = "../../sbf-inventario/tests/comum/mod.rs"]
mod comum;

use std::time::{Duration, Instant};

use sbf_inventario::banco::Inventario;
use sbf_inventario::consulta::{Area, Celula, Filtro, PedidoDeTabela};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::{Consulta, Ref};
use sbf_janela::{Destino, EstadoDaBarra, IdAcao, Janela, MolduraProvisoria, Pedido};

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
    moldura: MolduraProvisoria,
    leitor: LeitorSqlite,
    inventario: Inventario,
}

impl Real {
    fn novo(caminho: &std::path::Path) -> Self {
        let inventario = Inventario::abrir(caminho).expect("abrir");
        assert!(!inventario.plano().expect("plano").precisa_migrar());
        let leitor = LeitorSqlite::abrir(caminho).expect("leitor");
        let ctx = egui::Context::default();
        let mut janela = Janela::nova();
        janela.hoje = "2026-11-13".into();
        let moldura = MolduraProvisoria::nova(janela, EstadoDaBarra::default());
        moldura.preparar(&ctx);
        Self {
            ctx,
            moldura,
            leitor,
            inventario,
        }
    }

    fn quadro(&mut self) -> Duration {
        let inicio = Instant::now();
        let Real {
            ctx,
            moldura,
            leitor,
            inventario,
        } = self;
        let mut saida = ctx.run_ui(entrada(), |ui| moldura.ui(ui, leitor, inventario));
        saida.textures_delta.clear();
        inicio.elapsed()
    }

    fn abrir(&mut self, d: Destino) {
        self.moldura.janela.pedir(Pedido::Abrir(d));
        self.quadro();
        self.quadro();
    }
}

fn alvos_das_areas(leitor: &LeitorSqlite) -> Vec<Ref> {
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
    let alvos = alvos_das_areas(&r.leitor);
    assert!(alvos.len() >= 8, "poucos tipos ligados: {alvos:?}");
    for alvo in alvos {
        r.abrir(Destino::Item(alvo.clone()));
        for i in 0..sbf_janela::pagina::abas_de(alvo.tipo).len() {
            r.moldura.janela.centro.abas.insert(alvo.clone(), i);
            r.quadro();
        }
    }
}

#[test]
fn uma_decisao_pela_janela_grava_no_banco_e_o_desfazer_restaura() {
    let banco = comum::banco_de_teste();
    let mut r = Real::novo(&banco.caminho);
    r.abrir(Destino::Area(Area::ATriar));
    let antes = r.leitor.contagens().expect("contagens");
    let t = r
        .leitor
        .tabela(&PedidoDeTabela::Area {
            area: Area::ATriar,
            filtro: Filtro::default(),
        })
        .expect("a triar");
    let Some(alvo) = t.linhas.iter().find_map(|l| l.alvo.clone()) else {
        panic!("a fixture não tem achado a triar");
    };
    r.moldura.janela.centro.selecao.so(alvo);
    r.moldura.janela.executar(IdAcao::Documento);
    r.quadro();
    assert!(
        r.moldura.janela.mensagem.as_ref().is_some_and(|m| !m.erro),
        "{:?}",
        r.moldura.janela.mensagem
    );
    let depois = r.leitor.contagens().expect("contagens");
    assert_eq!(depois.a_triar, antes.a_triar - 1);
    r.moldura.janela.executar(IdAcao::Desfazer);
    r.quadro();
    assert_eq!(r.leitor.contagens().expect("contagens"), antes);
}

#[test]
fn com_50_mil_achados_a_area_desenha_rapido() {
    let banco = comum::banco_de_teste();
    {
        let conn = rusqlite::Connection::open(&banco.caminho).expect("abrir");
        comum::popular_grande(&conn, 50_000);
    }
    let mut r = Real::novo(&banco.caminho);
    r.moldura
        .janela
        .pedir(Pedido::Abrir(Destino::Area(Area::Documentos)));
    let primeiro = r.quadro() + r.quadro();
    let mut pior = Duration::ZERO;
    for _ in 0..5 {
        pior = pior.max(r.quadro());
    }
    eprintln!("primeiro quadro (com a consulta): {primeiro:?}; quadro seguinte (cache): {pior:?}");
    assert!(
        pior < Duration::from_millis(100),
        "quadro com cache lento: {pior:?}"
    );
    assert!(
        primeiro < Duration::from_secs(3),
        "abrir a área lento: {primeiro:?}"
    );
}
