//! PA-01 (uma fixture fictícia por regra), PA-02 (deduplicação) e PA-03 (estimativa marcada).
//! Hoje, nos testes, é 2026-11-14; o ciclo atual vai de 2026-10-20 a 2026-11-19.

#[path = "../../sbf-inventario/tests/comum/mod.rs"]
mod comum;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use sbf_inventario::banco::Inventario;
use sbf_livro::Data;
use sbf_pontos::regras::{custo_ia, dinheiro, operacao, tempo};
use sbf_pontos::{
    Analise, Area, Armazem, Candidato, Contexto, Estado, Filtro, Limites, Unidade, analisar,
};

const AGORA: &str = "2026-11-14T12:00:00Z";

fn hoje() -> Data {
    Data::nova(2026, 11, 14).expect("data")
}

struct Cenario {
    dir: tempfile::TempDir,
    inv: Connection,
}

fn cenario() -> Cenario {
    let dir = tempfile::tempdir().expect("dir");
    let caminho = dir.path().join("inventario.sqlite");
    let c = Connection::open(&caminho).expect("abrir");
    comum::aplicar_schema_do_bun(&c);
    drop(c);
    let mut inv = Inventario::abrir(&caminho).expect("abrir");
    inv.migrar(dir.path().join("copia.sqlite")).expect("migrar");
    drop(inv);
    let inv = Connection::open(&caminho).expect("reabrir");
    for (f, tipo) in [("gmail", "email"), ("disco:e2", "disco")] {
        inv.execute(
            "INSERT INTO fonte (id, tipo, descricao) VALUES (?1, ?2, ?1)",
            [f, tipo],
        )
        .expect("fonte");
    }
    Cenario { dir, inv }
}

/// (id, tipo, valor, data, fornecedor, descricao, documento)
type Mov<'a> = (
    &'a str,
    &'a str,
    i64,
    &'a str,
    Option<&'a str>,
    &'a str,
    Option<&'a str>,
);

fn cache(pasta: &Path, movs: &[Mov<'_>]) -> PathBuf {
    let caminho = pasta.join("livro-caixa.sqlite");
    let c = Connection::open(&caminho).expect("cache");
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS linha (colecao TEXT NOT NULL, id TEXT NOT NULL, versao TEXT,
           dados TEXT NOT NULL, PRIMARY KEY (colecao, id));",
    )
    .expect("tabela");
    for (col, id, dados) in [
        (
            "fornecedores",
            "f1",
            r#"{"id":"f1","nome":"Filmes Exemplo"}"#,
        ),
        (
            "fornecedores",
            "f2",
            r#"{"id":"f2","nome":"Seguro Exemplo"}"#,
        ),
        (
            "fornecedores",
            "f3",
            r#"{"id":"f3","nome":"Revista Exemplo"}"#,
        ),
    ] {
        c.execute(
            "INSERT OR REPLACE INTO linha (colecao, id, dados) VALUES (?1, ?2, ?3)",
            params![col, id, dados],
        )
        .expect("f");
    }
    for (id, tipo, valor, data, fornecedor, descricao, documento) in movs {
        let dados = serde_json::json!({
            "id": id, "tipo": tipo, "valor": valor, "data": data, "fornecedor": fornecedor,
            "descricao": descricao, "documento": documento
        });
        c.execute(
            "INSERT OR REPLACE INTO linha (colecao, id, dados) VALUES ('movimentacoes', ?1, ?2)",
            params![id, dados.to_string()],
        )
        .expect("mov");
    }
    caminho
}

fn analisar_com(
    c: &Cenario,
    cache: Option<&Path>,
    ajuste: impl FnOnce(&mut Contexto<'_>),
) -> Analise {
    let mut ctx = Contexto::novo(&c.inv, hoje());
    ctx.cache_do_livro = cache;
    ajuste(&mut ctx);
    analisar(&ctx, &Limites::default()).expect("analisar")
}

fn da_regra<'a>(a: &'a Analise, regra: &str) -> Vec<&'a Candidato> {
    a.candidatos.iter().filter(|c| c.regra == regra).collect()
}

// -- Dinheiro -------------------------------------------------------------------------------

#[test]
fn pa01_assinatura_que_subiu_de_preco() {
    let c = cenario();
    let cache = cache(
        c.dir.path(),
        &[
            (
                "m1",
                "saida",
                3990,
                "2026-09-05",
                Some("f1"),
                "Assinatura",
                None,
            ),
            (
                "m2",
                "saida",
                3990,
                "2026-10-05",
                Some("f1"),
                "Assinatura",
                None,
            ),
            (
                "m3",
                "saida",
                5490,
                "2026-11-05",
                Some("f1"),
                "Assinatura",
                None,
            ),
        ],
    );
    let a = analisar_com(&c, Some(&cache), |_| {});
    let r = da_regra(&a, dinheiro::REAJUSTE);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].area, Area::Dinheiro);
    assert!(r[0].titulo.contains("Filmes Exemplo"));
    let i = r[0].impacto.as_ref().expect("impacto");
    assert_eq!(
        (i.valor, i.unidade, i.estimativa),
        (1500 * 12, Unidade::CentavosBrl, true)
    );
    assert_eq!(r[0].evidencia.len(), 3);
    assert!(r[0].evidencia.iter().all(|e| e.tipo == "movimentacao"));
    assert!(
        da_regra(&a, dinheiro::SEM_DOCUMENTO).is_empty(),
        "valores diferentes demais"
    );
}

#[test]
fn pa01_cobranca_recorrente_sem_documento() {
    let c = cenario();
    let cache = cache(
        c.dir.path(),
        &[
            (
                "a1",
                "saida",
                2990,
                "2026-09-10",
                Some("f2"),
                "Seguro",
                None,
            ),
            (
                "a2",
                "saida",
                2990,
                "2026-10-10",
                Some("f2"),
                "Seguro",
                None,
            ),
            (
                "a3",
                "saida",
                2990,
                "2026-11-10",
                Some("f2"),
                "Seguro",
                None,
            ),
            // Mesma frequência, mas uma cobrança tem documento: não dispara.
            (
                "b1",
                "saida",
                1500,
                "2026-09-12",
                Some("f3"),
                "Revista",
                None,
            ),
            (
                "b2",
                "saida",
                1500,
                "2026-10-12",
                Some("f3"),
                "Revista",
                Some("doc-1"),
            ),
            (
                "b3",
                "saida",
                1500,
                "2026-11-12",
                Some("f3"),
                "Revista",
                None,
            ),
        ],
    );
    let a = analisar_com(&c, Some(&cache), |_| {});
    let r = da_regra(&a, dinheiro::SEM_DOCUMENTO);
    assert_eq!(r.len(), 1);
    assert!(r[0].titulo.contains("Seguro Exemplo"));
    assert_eq!(
        r[0].impacto.as_ref().map(|i| (i.valor, i.estimativa)),
        Some((2990 * 12, true))
    );
    assert!(da_regra(&a, dinheiro::REAJUSTE).is_empty());
}

#[test]
fn pa01_tarifas_somadas_no_ciclo() {
    let c = cenario();
    let cache = cache(
        c.dir.path(),
        &[
            (
                "t1",
                "tarifa",
                1990,
                "2026-10-25",
                None,
                "Pacote de serviços",
                None,
            ),
            ("t2", "tarifa", 1590, "2026-11-03", None, "Tarifa TED", None),
            // Fora do ciclo atual: não conta.
            (
                "t0",
                "tarifa",
                9990,
                "2026-10-01",
                None,
                "Tarifa antiga",
                None,
            ),
        ],
    );
    let a = analisar_com(&c, Some(&cache), |_| {});
    let r = da_regra(&a, dinheiro::TARIFAS);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].chave, "tarifas-no-ciclo:2026-10");
    let i = r[0].impacto.as_ref().expect("impacto");
    assert_eq!(
        (i.valor, i.estimativa, i.periodo.as_str()),
        (3580, false, "ciclo")
    );
    assert_eq!(r[0].evidencia.len(), 2);
}

#[test]
fn pa01_juros_e_multa_pagos() {
    let c = cenario();
    let cache = cache(
        c.dir.path(),
        &[
            (
                "j1",
                "juros",
                1234,
                "2026-11-02",
                None,
                "Juros do cartão",
                None,
            ),
            (
                "j2",
                "saida",
                500,
                "2026-11-04",
                None,
                "Multa por atraso",
                None,
            ),
            ("j3", "saida", 8000, "2026-11-04", None, "Mercado", None),
        ],
    );
    let a = analisar_com(&c, Some(&cache), |_| {});
    let r = da_regra(&a, dinheiro::JUROS);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].impacto.as_ref().map(|i| i.valor), Some(1734));
    assert_eq!(r[0].evidencia.len(), 2);
}

#[test]
fn sem_cache_as_regras_de_dinheiro_nao_rodam() {
    let c = cenario();
    let a = analisar_com(&c, None, |_| {});
    assert!(!a.avaliadas.contains(&dinheiro::TARIFAS));
    let a = analisar_com(&c, Some(&c.dir.path().join("nao-existe.sqlite")), |_| {});
    assert!(!a.avaliadas.contains(&dinheiro::TARIFAS));
}

// -- Operação -------------------------------------------------------------------------------

fn achado(c: &Connection, n: u8, estado: &str, visto: &str) -> String {
    let h = comum::hash(n);
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, primeiro_visto_em,
           classificado_por, decidido_por) VALUES (?1, 1, 'A', ?2, ?3, 't', 't')",
        params![h, estado, visto],
    )
    .expect("achado");
    h
}

#[test]
fn pa01_documentos_parados_ha_mais_de_30_dias() {
    let c = cenario();
    achado(&c.inv, 1, "novo", "2026-09-01T10:00:00.000Z");
    achado(&c.inv, 2, "novo", "2026-09-20T10:00:00.000Z");
    achado(&c.inv, 3, "novo", "2026-11-10T10:00:00.000Z"); // recente: não conta
    achado(&c.inv, 4, "duvida", "2026-08-15T10:00:00.000Z");
    achado(&c.inv, 5, "documento", "2026-01-01T10:00:00.000Z"); // já decidido: não conta
    let a = analisar_com(&c, None, |_| {});
    let r = da_regra(&a, operacao::DOCUMENTOS_PARADOS);
    assert_eq!(r.len(), 2);
    let a_triar = r.iter().find(|x| x.chave == "a-triar").expect("a triar");
    assert!(a_triar.titulo.starts_with("2 documentos"));
    assert_eq!(a_triar.evidencia.len(), 2);
    let duvida = r
        .iter()
        .find(|x| x.chave == "em-duvida")
        .expect("em dúvida");
    assert!(duvida.titulo.starts_with("1 documentos"));
}

#[test]
fn pa01_coleta_que_falha_repetidamente() {
    let c = cenario();
    for (fonte, status) in [
        ("gmail", "falhou"),
        ("gmail", "falhou"),
        ("gmail", "falhou"),
        ("disco:e2", "falhou"),
        ("disco:e2", "falhou"),
        ("disco:e2", "concluida"),
    ] {
        c.inv
            .execute(
                "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, status)
                 VALUES (?1, 'E1', 'x', 't', ?2)",
                params![fonte, status],
            )
            .expect("varredura");
    }
    let a = analisar_com(&c, None, |_| {});
    let r = da_regra(&a, operacao::COLETA_FALHANDO);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].chave, "gmail");
    assert_eq!(r[0].evidencia.len(), 3);
}

#[test]
fn pa01_disco_sem_cifra() {
    let c = cenario();
    let a = analisar_com(&c, None, |x| x.disco_cifrado = Some(false));
    assert_eq!(da_regra(&a, operacao::DISCO_SEM_CIFRA).len(), 1);
    let a = analisar_com(&c, None, |x| x.disco_cifrado = Some(true));
    assert!(da_regra(&a, operacao::DISCO_SEM_CIFRA).is_empty());
    let a = analisar_com(&c, None, |_| {});
    assert!(
        !a.avaliadas.contains(&operacao::DISCO_SEM_CIFRA),
        "desconhecido não conclui"
    );
}

// -- Custo de IA ----------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn saida(c: &Connection, provedor: &str, quando: &str, tokens: i64, custo: i64, nominal: bool) {
    c.execute(
        "INSERT INTO saida_ia (destinatario, provedor, modelo, nivel, bytes_enviados, tokens_entrada,
           tokens_saida, custo_dezmilesimos_usd, nominal, quando)
         VALUES ('triagem', ?1, 'm', 'completo', 100, ?3, 0, ?4, ?5, ?2)",
        params![provedor, quando, tokens, custo, nominal],
    )
    .expect("saida");
}

#[test]
fn pa01_consumo_de_ia_acima_do_ritmo() {
    let c = cenario();
    // Quatro semanas anteriores: 10 mil tokens por semana. Semana atual: 120 mil.
    for dia in ["2026-10-12", "2026-10-19", "2026-10-26", "2026-11-02"] {
        saida(
            &c.inv,
            "claude-cli",
            &format!("{dia}T10:00:00Z"),
            10_000,
            0,
            true,
        );
    }
    saida(
        &c.inv,
        "openrouter",
        "2026-11-10T10:00:00Z",
        60_000,
        25_000,
        false,
    );
    saida(
        &c.inv,
        "claude-cli",
        "2026-11-12T10:00:00Z",
        60_000,
        0,
        true,
    );
    let a = analisar_com(&c, None, |_| {});
    let r = da_regra(&a, custo_ia::CONSUMO_ACIMA_DO_RITMO);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].area, Area::CustoDeIa);
    let i = r[0].impacto.as_ref().expect("custo real da semana");
    assert_eq!(
        (i.valor, i.unidade, i.estimativa),
        (250, Unidade::CentavosUsd, false)
    );
}

#[test]
fn consumo_normal_nao_gera_ponto() {
    let c = cenario();
    for dia in ["2026-10-12", "2026-10-26", "2026-11-02", "2026-11-11"] {
        saida(
            &c.inv,
            "claude-cli",
            &format!("{dia}T10:00:00Z"),
            60_000,
            0,
            true,
        );
    }
    let a = analisar_com(&c, None, |_| {});
    assert!(da_regra(&a, custo_ia::CONSUMO_ACIMA_DO_RITMO).is_empty());
}

#[test]
fn pa01_provedor_reserva_em_uso() {
    let c = cenario();
    saida(
        &c.inv,
        "openrouter",
        "2026-11-12T10:00:00Z",
        1000,
        400,
        false,
    );
    saida(&c.inv, "claude-cli", "2026-11-12T11:00:00Z", 1000, 0, true);
    let a = analisar_com(&c, None, |x| x.provedor_reserva = Some("openrouter"));
    let r = da_regra(&a, custo_ia::PROVEDOR_RESERVA);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].impacto.as_ref().map(|i| i.valor), Some(4));
    // Sem reserva configurada, a regra não roda.
    let a = analisar_com(&c, None, |_| {});
    assert!(da_regra(&a, custo_ia::PROVEDOR_RESERVA).is_empty());
}

// -- Tempo do usuário -----------------------------------------------------------------------

#[test]
fn pa01_fila_de_sugestoes_crescendo() {
    let c = cenario();
    for i in 0..25 {
        c.inv
            .execute(
                "INSERT INTO sugestao_ia (alvo_tipo, alvo_id, campo, valor_json, confianca, justificativa,
                   modelo, prompt_versao, criada_em)
                 VALUES ('registro', ?1, 'categoria', '\"x\"', 0.9, 'j', 'm', 'v2', '2026-11-10T10:00:00.000Z')",
                [i.to_string()],
            )
            .expect("sugestao");
    }
    let a = analisar_com(&c, None, |_| {});
    let r = da_regra(&a, tempo::FILA_CRESCENDO);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].area, Area::TempoDoUsuario);
    let i = r[0].impacto.as_ref().expect("minutos");
    assert_eq!((i.unidade, i.estimativa), (Unidade::Minutos, true));
}

#[test]
fn o_motivo_do_aceite_confere_com_o_da_triagem() {
    assert_eq!(
        tempo::MOTIVO_DO_ACEITE,
        sbf_triagem::aceite::MOTIVO_DA_DECISAO
    );
}

#[test]
fn inventario_vazio_nao_gera_nada() {
    let c = cenario();
    let a = analisar_com(&c, None, |_| {});
    assert!(a.candidatos.is_empty());
}

// -- PA-02 e PA-03 --------------------------------------------------------------------------

fn tarifas(pasta: &Path, extra: bool) -> PathBuf {
    let mut m: Vec<Mov<'_>> = vec![
        ("t1", "tarifa", 1990, "2026-10-25", None, "Pacote", None),
        ("t2", "tarifa", 1590, "2026-11-03", None, "TED", None),
    ];
    if extra {
        m.push(("t3", "tarifa", 800, "2026-11-10", None, "Extrato", None));
    }
    cache(pasta, &m)
}

#[test]
fn pa02_ponto_aberto_nao_se_repete() {
    let c = cenario();
    let cache = tarifas(c.dir.path(), false);
    let a = analisar_com(&c, Some(&cache), |_| {});
    let mut z = Armazem::em_memoria().expect("armazém");
    let r1 = z.aplicar(&a.candidatos, &a.avaliadas, AGORA).expect("1");
    assert_eq!((r1.novos, r1.reabertos), (1, 0));
    let r2 = z
        .aplicar(&a.candidatos, &a.avaliadas, "2026-11-15T12:00:00Z")
        .expect("2");
    assert_eq!((r2.novos, r2.reabertos, r2.atualizados), (0, 0, 0));
    assert_eq!(z.listar(Filtro::default()).expect("lista").len(), 1);
}

#[test]
fn pa02_ignorado_so_volta_se_a_evidencia_mudar() {
    let c = cenario();
    let cache1 = tarifas(c.dir.path(), false);
    let a1 = analisar_com(&c, Some(&cache1), |_| {});
    let mut z = Armazem::em_memoria().expect("armazém");
    z.aplicar(&a1.candidatos, &a1.avaliadas, AGORA).expect("1");
    let id = z.listar(Filtro::default()).expect("lista")[0].id;

    assert!(
        z.ignorar(id, "  ", AGORA).is_err(),
        "o motivo é obrigatório"
    );
    z.ignorar(id, "tarifa do pacote que eu já negociei", AGORA)
        .expect("ignorar");
    let p = z.ponto(id).expect("ponto").expect("existe");
    assert_eq!(
        (p.estado, p.motivo.as_deref()),
        (
            Estado::Ignorado,
            Some("tarifa do pacote que eu já negociei")
        )
    );

    // Mesma evidência: continua ignorado.
    let r = z
        .aplicar(&a1.candidatos, &a1.avaliadas, "2026-11-15T12:00:00Z")
        .expect("2");
    assert_eq!((r.novos, r.reabertos), (0, 0));
    assert_eq!(z.ponto(id).expect("p").expect("e").estado, Estado::Ignorado);

    // Chegou outra tarifa: a evidência mudou, o ponto reabre (sem o motivo antigo) e é o mesmo.
    let cache2 = tarifas(c.dir.path(), true);
    let a2 = analisar_com(&c, Some(&cache2), |_| {});
    let r = z
        .aplicar(&a2.candidatos, &a2.avaliadas, "2026-11-16T12:00:00Z")
        .expect("3");
    assert_eq!((r.novos, r.reabertos), (0, 1));
    let p = z.ponto(id).expect("p").expect("e");
    assert_eq!((p.estado, p.motivo), (Estado::Aberto, None));
    assert_eq!(p.evidencia.len(), 3);
    assert_eq!(z.listar(Filtro::default()).expect("l").len(), 1);
}

#[test]
fn pa02_resolvido_a_mao_fica_resolvido_e_o_que_deixa_de_valer_fecha_sozinho() {
    let c = cenario();
    let cache = tarifas(c.dir.path(), false);
    let a = analisar_com(&c, Some(&cache), |_| {});
    let mut z = Armazem::em_memoria().expect("armazém");
    z.aplicar(&a.candidatos, &a.avaliadas, AGORA).expect("1");
    let id = z.listar(Filtro::default()).expect("l")[0].id;
    z.resolver(id, "troquei de pacote", AGORA)
        .expect("resolver");
    z.aplicar(&a.candidatos, &a.avaliadas, AGORA).expect("2");
    assert_eq!(
        z.ponto(id).expect("p").expect("e").estado,
        Estado::Resolvido
    );
    assert!(z.resolver(id, "de novo", AGORA).is_err(), "já está fechado");

    // Outro ponto: aberto, depois a evidência some e a regra roda: fecha sozinho.
    let mut z = Armazem::em_memoria().expect("armazém");
    z.aplicar(&a.candidatos, &a.avaliadas, AGORA).expect("1");
    z.aplicar(&[], &a.avaliadas, "2026-11-20T12:00:00Z")
        .expect("2");
    let p = &z.listar(Filtro::default()).expect("l")[0];
    assert_eq!(
        (p.estado, p.motivo.as_deref()),
        (Estado::Resolvido, Some("deixou de valer"))
    );
    // Se a regra nem rodou (sem cache), não fecha.
    let mut z = Armazem::em_memoria().expect("armazém");
    z.aplicar(&a.candidatos, &a.avaliadas, AGORA).expect("1");
    z.aplicar(&[], &[], "2026-11-20T12:00:00Z").expect("2");
    assert_eq!(
        z.listar(Filtro::default()).expect("l")[0].estado,
        Estado::Aberto
    );
}

#[test]
fn filtro_por_area_e_estado_e_aviso_so_uma_vez() {
    let c = cenario();
    let cache = tarifas(c.dir.path(), false);
    achado(&c.inv, 1, "novo", "2026-09-01T10:00:00.000Z");
    let a = analisar_com(&c, Some(&cache), |x| x.disco_cifrado = Some(false));
    let mut z = Armazem::em_memoria().expect("armazém");
    z.aplicar(&a.candidatos, &a.avaliadas, AGORA)
        .expect("aplicar");
    let din = z
        .listar(Filtro {
            area: Some(Area::Dinheiro),
            estado: None,
        })
        .expect("l");
    let ope = z
        .listar(Filtro {
            area: Some(Area::Operacao),
            estado: Some(Estado::Aberto),
        })
        .expect("l");
    assert_eq!((din.len(), ope.len()), (1, 2));
    assert_eq!(
        z.abertos_por_area().expect("n").get(&Area::Operacao),
        Some(&2)
    );

    let novos = z.para_avisar().expect("novos");
    assert_eq!(novos.len(), 3);
    z.marcar_avisados(&novos.iter().map(|p| p.id).collect::<Vec<_>>(), AGORA)
        .expect("marcar");
    assert!(z.para_avisar().expect("novos").is_empty());
}

#[test]
fn pa03_toda_projecao_vem_marcada_como_estimativa() {
    let c = cenario();
    let cache = cache(
        c.dir.path(),
        &[
            ("m1", "saida", 3990, "2026-09-05", Some("f1"), "A", None),
            ("m2", "saida", 3990, "2026-10-05", Some("f1"), "A", None),
            ("m3", "saida", 5490, "2026-11-05", Some("f1"), "A", None),
            ("a1", "saida", 2990, "2026-09-10", Some("f2"), "S", None),
            ("a2", "saida", 2990, "2026-10-10", Some("f2"), "S", None),
            ("a3", "saida", 2990, "2026-11-10", Some("f2"), "S", None),
            ("j1", "juros", 1234, "2026-11-02", None, "Juros", None),
        ],
    );
    let a = analisar_com(&c, Some(&cache), |_| {});
    assert_eq!(a.candidatos.len(), 3);
    for cand in &a.candidatos {
        let i = cand.impacto.as_ref().expect("impacto");
        let projecao = i.periodo == "ano";
        assert_eq!(
            i.estimativa, projecao,
            "{}: projeção é estimativa, apurado não",
            cand.regra
        );
        assert_eq!(i.texto().starts_with("estimativa: "), projecao);
    }
}
