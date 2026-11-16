//! Mede a abertura, a migração e as consultas sobre uma **cópia** de um inventário.
//!
//! Uso: `cargo run --release -p sbf-inventario --example medir -- <copia.sqlite>`
//!
//! Imprime só contagens e tempos, nunca conteúdo de linha: serve para conferir o
//! INV-01, o INV-02 e o INV-16 sobre o inventário real sem expor dado. Recusa abrir um
//! arquivo dentro da pasta do cofre, para nunca migrar o original por engano.

use std::path::PathBuf;
use std::time::Instant;

use sbf_inventario::banco::Inventario;
use sbf_inventario::consulta::{Area, Consulta, Filtro, Fonte, PedidoDeTabela, Pergunta};
use sbf_inventario::leitor::LeitorSqlite;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let caminho: PathBuf = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("uso: medir <copia.sqlite>")?;
    let absoluto = std::fs::canonicalize(&caminho)?;
    if absoluto.starts_with("/mnt/d/Chico/financas") {
        return Err("recuso: isto é o original do cofre; meça uma cópia".into());
    }

    let t = Instant::now();
    let mut inventario = Inventario::abrir(&absoluto)?;
    println!(
        "abrir: {:?} em {} ms",
        inventario.estado(),
        t.elapsed().as_millis()
    );
    let plano = inventario.plano()?;
    println!(
        "plano: da versão {} para a {}, {} passo(s), cópia antes: {}",
        plano.versao_atual,
        plano.versao_final,
        plano.passos.len(),
        plano.faz_copia
    );
    for passo in &plano.passos {
        println!("  {}: {}", passo.versao, passo.descricao);
    }
    if plano.precisa_migrar() {
        let t = Instant::now();
        let feita = inventario.migrar(inventario.caminho_sugerido_da_copia())?;
        println!(
            "migrar: {} → {} em {} ms (adotou o Bun: {})",
            feita.versao_inicial,
            feita.versao_final,
            t.elapsed().as_millis(),
            feita.adotou_o_bun
        );
    }
    drop(inventario);

    let t = Instant::now();
    let leitor = LeitorSqlite::abrir(&absoluto)?;
    println!("leitor: {} ms", t.elapsed().as_millis());

    let areas = [
        Area::ATriar,
        Area::EmDuvida,
        Area::Documentos,
        Area::Registros,
        Area::Ciclos,
        Area::Fornecedores,
        Area::Faturas,
        Area::ContasLacunas,
        Area::Emails,
        Area::EmailsPorRemetente,
        Area::ArquivosEPastas,
        Area::PastasPessoais,
        Area::Coletas,
        Area::Pendencias,
        Area::Lotes,
    ];
    for area in areas {
        let pedido = PedidoDeTabela::Area {
            area,
            filtro: Filtro::default(),
        };
        let t = Instant::now();
        let pagina = leitor.tabela_paginada(&pedido, 0, 1000)?;
        let primeira = t.elapsed().as_millis();
        let t = Instant::now();
        let ultima = leitor.tabela_paginada(&pedido, pagina.total.saturating_sub(1000), 1000)?;
        println!(
            "área {area:?}: {} linhas; 1ª página ({}) em {primeira} ms, última ({}) em {} ms",
            pagina.total,
            pagina.tabela.linhas.len(),
            ultima.tabela.linhas.len(),
            t.elapsed().as_millis()
        );
    }
    let filtro_texto = Filtro {
        texto: Some("a".to_owned()),
        ..Filtro::default()
    };
    for area in [Area::Documentos, Area::Registros, Area::Fornecedores] {
        let t = Instant::now();
        let tabela = leitor.tabela(&PedidoDeTabela::Area {
            area,
            filtro: filtro_texto.clone(),
        })?;
        println!(
            "filtro de texto em {area:?}: {} linhas em {} ms",
            tabela.linhas.len(),
            t.elapsed().as_millis()
        );
    }

    let t = Instant::now();
    let painel = leitor.visao_geral()?;
    println!(
        "visão geral: {} seções em {} ms",
        painel.secoes.len(),
        t.elapsed().as_millis()
    );
    let perguntas = [
        Pergunta::QuantoSaiu,
        Pergunta::MaioresFornecedores,
        Pergunta::OQueSeRepete,
        Pergunta::OQueAumentou,
        Pergunta::OndeSeConcentra,
        Pergunta::NaoClassificado,
        Pergunta::PossiveisDuplicidades,
        Pergunta::OQueFaltaParaConfiar,
    ];
    for pergunta in perguntas {
        let t = Instant::now();
        let painel = leitor.relatorio(pergunta, Fonte::Inventario, &Filtro::default())?;
        println!(
            "relatório {pergunta:?}: {} seções em {} ms",
            painel.secoes.len(),
            t.elapsed().as_millis()
        );
    }
    let t = Instant::now();
    let c = leitor.contagens()?;
    println!(
        "contagens em {} ms (achados {}, documentos {}, em dúvida {}, a triar {})",
        t.elapsed().as_millis(),
        c.achados,
        c.documentos,
        c.em_duvida,
        c.a_triar
    );
    Ok(())
}
