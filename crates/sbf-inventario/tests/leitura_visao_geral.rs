//! INV-04: a Visão geral mostra as mesmas contagens que o `relatorio.ts` para o mesmo banco.
//!
//! O teste roda o `relatorio.ts` com `bun` sobre a fixture (`FINANCAS_DIR` aponta para a
//! pasta temporária do teste: nunca para o cofre real) e compara o JSON com o que o Rust
//! mede. Sem `bun` no PATH nem em `~/.bun/bin`, a comparação direta fica UNVERIFIED e o
//! teste confere o mesmo banco contra consultas de referência.

mod comum;

use std::path::PathBuf;
use std::process::Command;

use rusqlite::Connection;
use sbf_inventario::consulta::{Celula, Consulta};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::visao_geral::{EstadoManual, VisaoGeral, hoje_iso, medir};
use sbf_inventario::vistas::Contexto;

fn achar_bun() -> Option<PathBuf> {
    let candidatos = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("bun"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .chain(std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".bun/bin/bun")));
    candidatos.into_iter().find(|c| c.is_file())
}

fn medir_no_rust(banco: &comum::BancoDeTeste) -> VisaoGeral {
    let conn = Connection::open(&banco.caminho).expect("conexão");
    let ctx = Contexto::novo(&conn).expect("contexto");
    medir(&ctx, &hoje_iso(), &EstadoManual::default()).expect("medir")
}

fn texto(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

#[test]
fn contagens_iguais_ao_relatorio_ts() {
    let banco = comum::banco_de_teste();
    let visao = medir_no_rust(&banco);

    let Some(bun) = achar_bun() else {
        eprintln!(
            "UNVERIFIED: bun não encontrado; INV-04 conferido só contra consultas de referência"
        );
        referencia_sem_bun(&banco, &visao);
        return;
    };
    let pasta = banco
        .caminho
        .parent()
        .expect("pasta do banco")
        .to_path_buf();
    let saida = pasta.join("painel-bun.json");
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let resultado = Command::new(bun)
        .current_dir(&raiz)
        .env("FINANCAS_DIR", &pasta)
        .args(["run", "workers/inventario/src/relatorio.ts", "--saida"])
        .arg(&saida)
        .output()
        .expect("rodar o relatorio.ts");
    assert!(
        resultado.status.success(),
        "relatorio.ts falhou: {}",
        String::from_utf8_lossy(&resultado.stderr)
    );
    let bun_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&saida).expect("saída")).expect("json");

    // Resumo.
    let r = &bun_json["resumo"];
    let n = |chave: &str| {
        r[chave]
            .as_i64()
            .unwrap_or_else(|| panic!("resumo.{chave}"))
    };
    assert_eq!(visao.resumo.achados, n("achados"));
    assert_eq!(visao.resumo.origens, n("origens"));
    assert_eq!(visao.resumo.copiados, n("copiados"));
    assert_eq!(visao.resumo.documentos, n("documentos"));
    assert_eq!(visao.resumo.registros, n("registros"));
    assert_eq!(visao.resumo.pendencias, n("pendencias"));
    assert_eq!(visao.resumo.fontes_concluidas, n("fontes_concluidas"));
    assert_eq!(visao.resumo.fontes_dentro, n("fontes_dentro"));
    assert!((visao.resumo.acervo_gb - r["acervo_gb"].as_f64().expect("acervo_gb")).abs() < 1e-9);

    // Fontes (a mesma ordem) e datas por fonte.
    let fontes = bun_json["fontes"].as_array().expect("fontes");
    assert_eq!(visao.fontes.len(), fontes.len());
    let datas = bun_json["datas"].as_array().expect("datas");
    for (rust, bun) in visao.fontes.iter().zip(fontes) {
        assert_eq!(Some(rust.id.clone()), texto(&bun["id"]));
        assert_eq!(
            Some(rust.status.clone()),
            texto(&bun["status"]),
            "{}",
            rust.id
        );
        assert_eq!(
            rust.vistos,
            bun["vistos"].as_i64().expect("vistos"),
            "{}",
            rust.id
        );
        assert_eq!(
            rust.novos,
            bun["novos"].as_i64().expect("novos"),
            "{}",
            rust.id
        );
        assert_eq!(rust.copiados, bun["copiados"].as_i64(), "{}", rust.id);
        assert_eq!(
            rust.erros,
            bun["erros"].as_i64().expect("erros"),
            "{}",
            rust.id
        );
        assert_eq!(rust.atualizado, texto(&bun["atualizado"]), "{}", rust.id);
        assert_eq!(rust.nota, texto(&bun["nota"]), "{}", rust.id);
        let d = datas
            .iter()
            .find(|d| texto(&d["id"]).as_deref() == Some(rust.id.as_str()))
            .expect("datas da fonte");
        assert_eq!(
            rust.itens,
            d["itens"].as_i64().expect("itens"),
            "{}",
            rust.id
        );
        assert_eq!(
            rust.periodo_itens,
            texto(&d["periodo_itens"]),
            "{}",
            rust.id
        );
        assert_eq!(
            rust.documentos,
            d["documentos"].as_i64().expect("documentos"),
            "{}",
            rust.id
        );
        assert_eq!(
            rust.periodo_documentos,
            texto(&d["periodo_documentos"]),
            "{}",
            rust.id
        );
    }

    // Áreas deixadas de fora.
    let areas = bun_json["areas"].as_array().expect("areas");
    assert_eq!(visao.areas.len(), areas.len());
    assert!(!areas.is_empty());
    for (rust, bun) in visao.areas.iter().zip(areas) {
        assert_eq!(Some(rust.motivo.clone()), texto(&bun["motivo"]));
        assert_eq!(rust.areas, bun["areas"].as_i64().expect("areas"));
        assert_eq!(rust.arquivos, bun["arquivos"].as_i64().expect("arquivos"));
        assert_eq!(rust.bytes, bun["bytes"].as_i64().expect("bytes"));
        assert!((rust.gb - bun["gb"].as_f64().expect("gb")).abs() < 1e-9);
    }

    // Os últimos 13 ciclos.
    let ciclos = bun_json["ciclos"].as_array().expect("ciclos");
    assert_eq!(visao.ciclos.len(), ciclos.len());
    assert_eq!(ciclos.len(), 13);
    for (rust, bun) in visao.ciclos.iter().zip(ciclos) {
        assert_eq!(Some(rust.ciclo.clone()), texto(&bun["ciclo"]));
        assert_eq!(Some(rust.periodo.clone()), texto(&bun["periodo"]));
        assert_eq!(
            rust.documentos,
            bun["documentos"].as_i64().expect("documentos")
        );
        assert_eq!(
            rust.registros,
            bun["registros"].as_i64().expect("registros")
        );
        assert_eq!(Some(rust.situacao.clone()), texto(&bun["situacao"]));
    }

    // Avisos: os de erro de leitura.
    let avisos = bun_json["avisos"].as_array().expect("avisos");
    let do_bun: Vec<String> = avisos.iter().filter_map(|a| texto(&a["aviso"])).collect();
    let do_rust: Vec<String> = visao.avisos.iter().map(|a| a.aviso.clone()).collect();
    assert_eq!(do_rust, do_bun);
    assert!(!do_rust.is_empty(), "a fixture tem uma fonte com erros");
}

/// Sem `bun`: as mesmas contagens, por consultas de referência escritas à parte.
fn referencia_sem_bun(banco: &comum::BancoDeTeste, visao: &VisaoGeral) {
    let conn = Connection::open(&banco.caminho).expect("conexão");
    let um = |sql: &str| -> i64 { conn.query_row(sql, [], |l| l.get(0)).expect("consulta") };
    assert_eq!(visao.resumo.achados, um("SELECT count(*) FROM achado"));
    assert_eq!(visao.resumo.origens, um("SELECT count(*) FROM origem"));
    assert_eq!(
        visao.resumo.documentos,
        um("SELECT count(*) FROM achado WHERE estado='documento'")
    );
    assert_eq!(visao.resumo.registros, um("SELECT count(*) FROM registro"));
    assert_eq!(
        visao.resumo.pendencias,
        um("SELECT count(*) FROM pendencia WHERE resolvida_em IS NULL")
    );
    assert_eq!(visao.ciclos.len(), 13);
}

#[test]
fn painel_tem_resumo_etapas_fontes_ciclos_e_avisos() {
    let banco = comum::banco_de_teste();
    let leitor = LeitorSqlite::abrir(&banco.caminho).expect("abrir");
    let painel = leitor.visao_geral().expect("painel");
    let titulos: Vec<&str> = painel.secoes.iter().map(|s| s.titulo.as_str()).collect();
    assert_eq!(
        titulos,
        [
            "Resumo",
            "Etapas",
            "Fontes",
            "Áreas deixadas de fora",
            "Últimos 13 ciclos"
        ]
    );
    assert!(
        painel
            .avisos
            .iter()
            .any(|a| a.contains("Erros de leitura em"))
    );
    // Etapas E0 a E5.
    assert_eq!(painel.secoes[1].tabela.linhas.len(), 6);
    // Cada fonte é um link para a conexão.
    for l in &painel.secoes[2].tabela.linhas {
        assert!(matches!(l.celulas[0], Celula::Link { .. }));
    }
    // Os ciclos: o atual vem primeiro e marcado.
    let ciclos = &painel.secoes[4].tabela;
    assert_eq!(ciclos.linhas.len(), 13);
    assert!(matches!(&ciclos.linhas[0].celulas[4], Celula::Texto(t) if t == "atual"));
}

#[test]
fn estado_manual_de_etapas_e_avisos_entra_no_painel() {
    let banco = comum::banco_de_teste();
    let pasta = banco.caminho.parent().expect("pasta");
    let estado = pasta.join("estado.json");
    std::fs::write(
        &estado,
        r#"{"etapas":{"E2":{"status":"rodando","detalhe":"3 de 5 fontes"}},"tarefasFeitas":["t01"],
            "avisos":[{"nivel":"info","aviso":"OneDrive ainda não conectado","desde":"2026-11-13"}]}"#,
    )
    .expect("estado");
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_estado_manual(EstadoManual::ler(&estado));
    let painel = leitor.visao_geral().expect("painel");
    assert!(
        painel
            .avisos
            .iter()
            .any(|a| a.contains("OneDrive ainda não conectado"))
    );
    let e2 = &painel.secoes[1].tabela.linhas[2];
    assert!(matches!(&e2.celulas[2], Celula::Texto(t) if t == "rodando"));
    assert!(matches!(&e2.celulas[3], Celula::Texto(t) if t == "3 de 5 fontes"));
}
