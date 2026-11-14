//! RG-01, RG-02, RG-03 e as fontes do índice, com dados fictícios e o embedder falso.

#[path = "../../sbf-inventario/tests/comum/mod.rs"]
mod comum;

use std::path::Path;
use std::sync::Arc;

use rusqlite::{Connection, params};
use sbf_ia::{Consumo, Falso, NivelDeConfianca, Resposta};
use sbf_indice::fontes::{atualizar, trechos_das_movimentacoes};
use sbf_indice::perguntar::{DESTINATARIO, perguntar};
use sbf_indice::{Busca, BuscaLocal, Embedder, EmbedderFalso, Indice, ItemRef, Trecho};
use sbf_inventario::banco::Inventario;
use sbf_inventario::sugestao::RegistroDaIa;

fn trecho(id: &str, texto: &str) -> Trecho {
    Trecho {
        id: id.into(),
        item: ItemRef::novo("achado", id),
        texto: texto.into(),
    }
}

fn corpus() -> Vec<Trecho> {
    vec![
        trecho(
            "a",
            "fatura de energia elétrica da Cemig vencimento dia dez",
        ),
        trecho("b", "mensalidade da academia Corpo Exemplo"),
        trecho("c", "boleto do condomínio Edifício Fictício"),
        trecho("d", "seguro do carro Seguradora Exemplo parcela"),
        trecho("e", "assinatura de streaming Filmes Exemplo"),
    ]
}

#[test]
fn rg01_o_falso_e_deterministico_e_nao_precisa_de_modelo() {
    let e = EmbedderFalso::novo();
    let a = e.embutir_trechos(&["conta de luz".into()]).expect("a");
    let b = EmbedderFalso::novo()
        .embutir_trechos(&["conta de luz".into()])
        .expect("b");
    assert_eq!(a, b);
    assert_eq!(e.nome(), "falso-256");
}

#[test]
fn rg02_reindexar_sem_mudanca_nao_recalcula_embeddings() {
    let e = EmbedderFalso::novo();
    let mut ind = Indice::em_memoria().expect("índice");
    let r1 = ind.sincronizar("achado", &corpus(), &e).expect("1");
    assert_eq!((r1.embutidos, r1.mantidos, r1.removidos), (5, 0, 0));
    let antes = e.embutidos();
    let r2 = ind.sincronizar("achado", &corpus(), &e).expect("2");
    assert_eq!((r2.embutidos, r2.mantidos, r2.removidos), (0, 5, 0));
    assert_eq!(e.embutidos(), antes, "nenhum embedding novo");
    // Muda um, tira outro: só o mudado é embutido.
    let mut novo = corpus();
    novo[0].texto = "fatura de água Copasa".into();
    novo.pop();
    let r3 = ind.sincronizar("achado", &novo, &e).expect("3");
    assert_eq!((r3.embutidos, r3.mantidos, r3.removidos), (1, 3, 1));
    assert_eq!(e.embutidos(), antes + 1);
    assert_eq!(ind.total().expect("total"), 4);
}

#[test]
fn trocar_de_modelo_refaz_os_vetores() {
    struct Outro(EmbedderFalso);
    impl Embedder for Outro {
        fn nome(&self) -> &str {
            "outro"
        }
        fn dimensao(&self) -> usize {
            self.0.dimensao()
        }
        fn embutir_trechos(&self, t: &[String]) -> sbf_indice::Resultado<Vec<Vec<f32>>> {
            self.0.embutir_trechos(t)
        }
    }
    let mut ind = Indice::em_memoria().expect("índice");
    ind.sincronizar("achado", &corpus(), &EmbedderFalso::novo())
        .expect("1");
    let r = ind
        .sincronizar("achado", &corpus(), &Outro(EmbedderFalso::novo()))
        .expect("2");
    assert_eq!(r.embutidos, 5);
}

#[test]
fn busca_devolve_o_mais_parecido_primeiro() {
    let e = EmbedderFalso::novo();
    let mut ind = Indice::em_memoria().expect("índice");
    ind.sincronizar("achado", &corpus(), &e).expect("sync");
    let q = e
        .embutir_consulta("quanto paguei de energia elétrica?")
        .expect("q");
    let r = ind.buscar(&q, 2).expect("buscar");
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].trecho_id, "a");
    assert!(r[0].nota >= r[1].nota);
}

#[test]
fn indice_em_arquivo_sobrevive_a_reabrir() {
    let pasta = tempfile::tempdir().expect("pasta");
    let caminho = pasta.path().join("dados/indice.sqlite");
    let e = EmbedderFalso::novo();
    {
        let mut ind = Indice::abrir(&caminho).expect("abrir");
        ind.sincronizar("achado", &corpus(), &e).expect("sync");
    }
    let ind = Indice::abrir(&caminho).expect("reabrir");
    assert_eq!(ind.total().expect("total"), 5);
}

// -- fontes --------------------------------------------------------------------------------

fn inventario_fictício() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("dir");
    let caminho = dir.path().join("inventario.sqlite");
    let c = Connection::open(&caminho).expect("abrir");
    comum::aplicar_schema_do_bun(&c);
    drop(c);
    let mut inv = Inventario::abrir(&caminho).expect("abrir");
    inv.migrar(dir.path().join("copia.sqlite")).expect("migrar");
    drop(inv);
    let c = Connection::open(&caminho).expect("reabrir");
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:e2', 'disco', 'd')",
        [],
    )
    .expect("fonte");
    c.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
         VALUES (1, 'disco:e2', 'E1', 'tudo', 'teste')",
        [],
    )
    .expect("varredura");
    (dir, c)
}

fn achado_com_texto(c: &Connection, n: u8, arquivo: &str, texto: &str) -> String {
    let h = comum::hash(n);
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, classificado_por, decidido_por)
         VALUES (?1, 10, 'A', 'documento', 'teste', 'teste')",
        [&h],
    )
    .expect("achado");
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
        params![h, texto],
    )
    .expect("texto");
    c.execute(
        "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, filename)
         VALUES (?1, 'disco:e2', 1, 'local_file', ?2, ?3)",
        params![h, format!("local:{arquivo}:{n}"), arquivo],
    )
    .expect("origem");
    h
}

fn cache_do_livro(pasta: &Path) -> std::path::PathBuf {
    let caminho = pasta.join("livro-caixa.sqlite");
    let c = Connection::open(&caminho).expect("cache");
    c.execute_batch("CREATE TABLE linha (colecao TEXT NOT NULL, id TEXT NOT NULL, versao TEXT, dados TEXT NOT NULL, PRIMARY KEY (colecao, id));")
        .expect("tabela");
    for (col, id, dados) in [
        (
            "fornecedores",
            "f1",
            r#"{"id":"f1","nome":"Mercado Exemplo"}"#,
        ),
        ("categorias", "c1", r#"{"id":"c1","nome":"Mercado"}"#),
        (
            "movimentacoes",
            "m1",
            r#"{"id":"m1","tipo":"saida","valor":18750,"descricao":"Compra do mês","data":"2026-10-05","fornecedor":"f1","categoria":"c1"}"#,
        ),
        (
            "movimentacoes",
            "m2",
            r#"{"id":"m2","tipo":"entrada","valor":500000,"descricao":null,"data":"2026-10-06","fornecedor":null,"categoria":null}"#,
        ),
    ] {
        c.execute(
            "INSERT INTO linha (colecao, id, dados) VALUES (?1, ?2, ?3)",
            params![col, id, dados],
        )
        .expect("linha");
    }
    caminho
}

#[test]
fn as_tres_fontes_entram_no_indice_e_a_atualizacao_e_incremental() {
    let (dir, inv) = inventario_fictício();
    let longo = "Fatura de energia da Cemig. ".repeat(80);
    achado_com_texto(&inv, 1, "fatura-energia.pdf", &longo);
    inv.execute(
        "INSERT INTO ciclo (id, inicio, fim) VALUES (?1, '2026-09-20', '2026-10-19')",
        [comum::ciclo_de("2026-10-03")],
    )
    .expect("ciclo");
    comum::inserir_registro(
        &inv,
        &comum::Reg {
            fonte: "disco:e2",
            tipo: "compra",
            data: "2026-10-03",
            valor: -2340,
            descricao: "Padaria Exemplo",
            fornecedor: Some("Padaria Exemplo"),
            ..comum::Reg::default()
        },
    );
    let cache = cache_do_livro(dir.path());
    let mov = trechos_das_movimentacoes(&cache).expect("mov");
    assert_eq!(
        mov.len(),
        1,
        "a movimentação sem descrição nem fornecedor fica de fora"
    );
    assert!(mov[0].texto.contains("Mercado Exemplo") && mov[0].texto.contains("R$ 187,50"));

    let e = EmbedderFalso::novo();
    let mut ind = Indice::em_memoria().expect("índice");
    let r = atualizar(&mut ind, &inv, Some(&cache), &e).expect("atualizar");
    assert!(
        r.embutidos >= 4,
        "achado longo em vários pedaços + registro + movimentação"
    );
    let antes = e.embutidos();
    let r2 = atualizar(&mut ind, &inv, Some(&cache), &e).expect("de novo");
    assert_eq!(r2.embutidos, 0);
    assert_eq!(e.embutidos(), antes);
    // Cache ausente não é erro.
    let r3 = atualizar(
        &mut ind,
        &inv,
        Some(&dir.path().join("nao-existe.sqlite")),
        &e,
    );
    assert!(r3.is_ok());
}

// -- Perguntar (RG-03) -----------------------------------------------------------------------

fn resposta_com(json: serde_json::Value) -> Resposta {
    Resposta {
        json,
        consumo: Consumo {
            tokens_entrada: 120,
            tokens_saida: 30,
            custo_dezmilesimos_usd: 0,
            nominal: true,
        },
        modelo: "falso-1".into(),
    }
}

#[test]
fn rg03_so_os_k_trechos_saem_e_o_registro_fica_em_saida_ia() {
    let (dir, inv) = inventario_fictício();
    let caminho_inv = dir.path().join("inventario.sqlite");
    // Cinco achados; só dois falam de energia.
    achado_com_texto(
        &inv,
        1,
        "luz-outubro.pdf",
        "Fatura de energia elétrica Cemig outubro total 210,00 CPF 123.456.789-09",
    );
    achado_com_texto(
        &inv,
        2,
        "luz-setembro.pdf",
        "Fatura de energia elétrica Cemig setembro total 198,00",
    );
    achado_com_texto(
        &inv,
        3,
        "academia.pdf",
        "Mensalidade da academia Corpo Exemplo",
    );
    achado_com_texto(
        &inv,
        4,
        "condominio.pdf",
        "Boleto do condomínio Edifício Fictício",
    );
    achado_com_texto(&inv, 5, "seguro.pdf", "Seguro do carro Seguradora Exemplo");
    let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let caminho_indice = dir.path().join("indice.sqlite");
    {
        let mut ind = Indice::abrir(&caminho_indice).expect("índice");
        atualizar(&mut ind, &inv, None, e.as_ref()).expect("atualizar");
    }
    let busca = BuscaLocal::nova(&caminho_indice, Arc::clone(&e));
    let falso = Falso::com_funcao(|_| {
        Ok(resposta_com(serde_json::json!({
            "resposta": "A fatura de outubro foi de R$ 210,00.",
            "citacoes": [1, 99]
        })))
    });
    let registro = RegistroDaIa::abrir(&caminho_inv, None).expect("registro");

    let r = perguntar(
        &busca,
        &falso,
        &registro,
        NivelDeConfianca::Mascarado,
        "quanto foi a fatura de energia elétrica da Cemig de outubro?",
        2,
    )
    .expect("perguntar");

    assert_eq!(r.trechos_enviados, 2);
    let pedido = &falso.pedidos()[0];
    let enviado = &pedido.entrada;
    assert!(enviado.contains("[1]") && enviado.contains("[2]") && !enviado.contains("[3]"));
    for fora in ["academia", "condomínio", "Seguradora"] {
        assert!(!enviado.contains(fora), "{fora} não podia sair");
    }
    assert!(
        !enviado.contains("123.456.789-09"),
        "CPF mascarado no nível padrão"
    );
    assert!(enviado.contains("•••"));
    // A citação 99 não existe entre os enviados e é descartada; a 1 vira link para o item.
    assert_eq!(r.citacoes.len(), 1);
    assert_eq!(r.citacoes[0].item.tipo, "achado");
    assert!(
        r.citacoes[0].trecho.contains("123.456.789-09"),
        "o trecho local fica inteiro"
    );

    let c = Connection::open(&caminho_inv).expect("abrir");
    let (dest, nivel, bytes, modelo): (String, String, i64, String) = c
        .query_row(
            "SELECT destinatario, nivel, bytes_enviados, modelo FROM saida_ia",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
        )
        .expect("uma saída");
    assert_eq!(
        (dest.as_str(), nivel.as_str(), modelo.as_str()),
        (DESTINATARIO, "mascarado", "falso-1")
    );
    assert!(usize::try_from(bytes).unwrap_or(0) >= enviado.len());
}

#[test]
fn sem_trecho_nada_sai_e_nada_e_registrado() {
    let (dir, _inv) = inventario_fictício();
    let caminho_inv = dir.path().join("inventario.sqlite");
    let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let busca = BuscaLocal::nova(dir.path().join("vazio.sqlite"), e);
    let falso = Falso::com_roteiro(vec![]);
    let registro = RegistroDaIa::abrir(&caminho_inv, None).expect("registro");
    let r = perguntar(
        &busca,
        &falso,
        &registro,
        NivelDeConfianca::Mascarado,
        "oi?",
        3,
    )
    .expect("ok");
    assert_eq!(r.trechos_enviados, 0);
    assert_eq!(falso.chamadas(), 0);
    let n: i64 = Connection::open(&caminho_inv)
        .expect("abrir")
        .query_row("SELECT COUNT(*) FROM saida_ia", [], |l| l.get(0))
        .expect("n");
    assert_eq!(n, 0);
}

#[test]
fn busca_exclui_o_proprio_item() {
    let dir = tempfile::tempdir().expect("dir");
    let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let caminho = dir.path().join("i.sqlite");
    Indice::abrir(&caminho)
        .expect("i")
        .sincronizar("achado", &corpus(), e.as_ref())
        .expect("sync");
    let busca = BuscaLocal::nova(&caminho, e);
    let r = busca
        .parecidos(
            "fatura de energia elétrica da Cemig",
            3,
            Some(&ItemRef::novo("achado", "a")),
        )
        .expect("busca");
    assert!(r.iter().all(|p| p.item.id != "a"));
    assert_eq!(r.len(), 3);
}

#[test]
fn provedor_que_falha_depois_de_receber_o_pedido_deixa_a_saida_registrada() {
    use sbf_ia::ErroDeIa;
    for (erro, saidas) in [
        (ErroDeIa::Outro("tempo esgotado".into()), 1_i64),
        (ErroDeIa::RespostaInvalida("não é JSON".into()), 1),
        (ErroDeIa::Indisponivel("claude não instalado".into()), 0),
        (ErroDeIa::Teto, 0),
    ] {
        let (dir, inv) = inventario_fictício();
        let caminho_inv = dir.path().join("inventario.sqlite");
        achado_com_texto(
            &inv,
            1,
            "luz.pdf",
            "Fatura de energia elétrica Cemig outubro total 210,00",
        );
        let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
        let caminho_indice = dir.path().join("indice.sqlite");
        {
            let mut ind = Indice::abrir(&caminho_indice).expect("índice");
            atualizar(&mut ind, &inv, None, e.as_ref()).expect("atualizar");
        }
        let busca = BuscaLocal::nova(&caminho_indice, Arc::clone(&e));
        let rotulo = erro.to_string();
        let falso = Falso::com_roteiro(vec![Err(erro)]);
        let registro = RegistroDaIa::abrir(&caminho_inv, None).expect("registro");
        let r = perguntar(
            &busca,
            &falso,
            &registro,
            NivelDeConfianca::Mascarado,
            "quanto foi a fatura de energia?",
            2,
        );
        assert!(r.is_err());
        let n: i64 = Connection::open(&caminho_inv)
            .expect("abrir")
            .query_row("SELECT COUNT(*) FROM saida_ia", [], |l| l.get(0))
            .expect("n");
        assert_eq!(n, saidas, "{rotulo}");
    }
}
