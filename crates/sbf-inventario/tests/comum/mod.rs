//! Fixture compartilhada dos testes do `sbf-inventario`. Dono: agente de leitura.
//!
//! Um banco pequeno e **coerente**, todo fictício (INV-17), gerado pelo próprio teste:
//!
//! - 5 fornecedores com registros em 3 ciclos (`2026-08`, `2026-09`, `2026-10`);
//! - documentos com 2 origens de fontes diferentes (disco e Gmail; disco e OneDrive);
//! - achados em cada estado, decididos por `usuario` e por `regras@2`;
//! - movimentos entre contas (transferência, investimento, pagamento de fatura);
//! - duas faturas de cartão, uma que fecha e outra que não;
//! - lacunas de conta (uma conta sem registros em dois meses);
//! - um registro promovido (`movimentacao_id`) num lote aprovado e promovido, e um lote
//!   aberto com achados e registros;
//! - decisões (inclusive uma desfeita), pendências, regras de senha e tentativas;
//! - varreduras de várias fontes (concluída, falhou, fonte fora do escopo, fonte ausente).
//!
//! Uso nos testes de integração: `mod comum;` e depois [`banco_de_teste`] (arquivo num
//! diretório temporário) ou [`aplicar_esquema`] + [`popular`] sobre uma conexão sua.
#![allow(dead_code)]

use std::path::PathBuf;

use rusqlite::{Connection, params};
use sbf_inventario::banco::Inventario;
use sbf_inventario::esquema::{MIGRACAO_1_BASE_BUN, MIGRACAO_2_DECISOES_E_LOTES};
use tempfile::TempDir;

/// Um banco em arquivo, apagado quando o valor sai de escopo.
pub struct BancoDeTeste {
    _dir: TempDir,
    pub caminho: PathBuf,
}

/// Cria `inventario.sqlite` num diretório temporário, como o app o abre: um banco do Bun
/// "2" que `Inventario::abrir(..).migrar(..)` leva ao esquema do Rust, e então os dados da
/// fixture.
pub fn banco_de_teste() -> BancoDeTeste {
    let dir = tempfile::tempdir().expect("diretório temporário");
    let caminho = dir.path().join("inventario.sqlite");
    let conn = Connection::open(&caminho).expect("abrir banco de teste");
    aplicar_schema_do_bun(&conn);
    drop(conn);
    let mut inventario = Inventario::abrir(&caminho).expect("abrir o inventário");
    inventario
        .migrar(dir.path().join("copia-antes-da-migracao.sqlite"))
        .expect("migrar");
    drop(inventario);
    let conn = Connection::open(&caminho).expect("reabrir banco de teste");
    popular(&conn);
    drop(conn);
    BancoDeTeste { _dir: dir, caminho }
}

/// O que o `aplicarSchema` do Bun faz num banco novo: `schema.sql`, as colunas do
/// `garantirColuna` e `meta.schema_versao = '2'`.
pub fn aplicar_schema_do_bun(conn: &Connection) {
    conn.execute_batch(MIGRACAO_1_BASE_BUN).expect("migração 1");
    conn.execute_batch(
        "ALTER TABLE registro ADD COLUMN tipo_original TEXT;
         ALTER TABLE registro ADD COLUMN contraparte TEXT;
         ALTER TABLE registro ADD COLUMN contraparte_doc TEXT;
         ALTER TABLE registro ADD COLUMN data_compra TEXT;
         ALTER TABLE registro ADD COLUMN merchant_cnpj TEXT;
         ALTER TABLE registro ADD COLUMN fatura_ref TEXT;
         INSERT OR REPLACE INTO meta (chave, valor) VALUES ('schema_versao', '2');",
    )
    .expect("colunas do Bun");
}

/// Migração 1, colunas do Bun e migração 2 por `execute_batch`, numa conexão sua (sem o
/// trigger de `decisao`). Prefira [`banco_de_teste`].
pub fn aplicar_esquema(conn: &Connection) {
    aplicar_schema_do_bun(conn);
    conn.execute_batch(MIGRACAO_2_DECISOES_E_LOTES)
        .expect("migração 2");
}

/// O hash fictício de número `n`: 64 caracteres hexadecimais.
pub fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

// Hashes que os testes citam.
pub const H_ENERGIA_OUT: u8 = 1; // fatura de energia, ciclo 2026-10, duas origens (disco + Gmail)
pub const H_ENERGIA_SET: u8 = 2; // boleto de energia, ciclo 2026-09, duas origens (disco + OneDrive)
pub const H_MERCADO: u8 = 3; // comprovante do Mercado Delta, anexo do Gmail
pub const H_FATURA_CARTAO: u8 = 4; // fatura do cartão, protegida por senha
pub const H_A_TRIAR_1: u8 = 5;
pub const H_A_TRIAR_2: u8 = 6;
pub const H_DESCARTADO: u8 = 7;
pub const H_DUVIDA_REGRAS: u8 = 8;
pub const H_DUVIDA_USUARIO: u8 = 9;
pub const H_STREAMING_A: u8 = 10; // possível duplicidade com o 11
pub const H_STREAMING_B: u8 = 11;
pub const H_EML: u8 = 12; // a mensagem em si (e-mail)
pub const H_PASTA_PESSOAL: u8 = 13; // camada A, em pasta pessoal

/// Dados de um registro. O que não importa fica no padrão.
#[derive(Default, Clone)]
pub struct Reg<'a> {
    pub hash: Option<String>,
    pub external_ref: Option<&'a str>,
    pub fonte: &'a str,
    pub tipo: &'a str,
    pub data: &'a str,
    pub valor: i64,
    pub descricao: &'a str,
    pub fornecedor: Option<&'a str>,
    pub supplier_raw: Option<&'a str>,
    pub categoria: Option<&'a str>,
    pub conta: Option<&'a str>,
    pub contraparte: Option<&'a str>,
    pub data_compra: Option<&'a str>,
    pub fatura_ref: Option<&'a str>,
    pub estado: &'a str,
    pub movimentacao_id: Option<&'a str>,
    pub extraido_por: &'a str,
    pub decidido_por: Option<&'a str>,
}

/// Ciclo (AAAA-MM do início) de uma data ISO, com o dia de corte 20.
pub fn ciclo_de(data: &str) -> String {
    let ano: i32 = data[0..4].parse().expect("ano");
    let mes: i32 = data[5..7].parse().expect("mês");
    let dia: i32 = data[8..10].parse().expect("dia");
    let (a, m) = if dia < 20 {
        if mes == 1 {
            (ano - 1, 12)
        } else {
            (ano, mes - 1)
        }
    } else {
        (ano, mes)
    };
    format!("{a:04}-{m:02}")
}

pub fn inserir_registro(conn: &Connection, r: &Reg<'_>) -> i64 {
    let estado = if r.estado.is_empty() {
        "extraido"
    } else {
        r.estado
    };
    let extraido_por = if r.extraido_por.is_empty() {
        "pluggy@1"
    } else {
        r.extraido_por
    };
    let fonte = if r.fonte.is_empty() {
        "conexao:pluggy"
    } else {
        r.fonte
    };
    let decidido = r.decidido_por.unwrap_or(extraido_por);
    conn.execute(
        "INSERT INTO registro (content_hash, external_ref, fonte_id, tipo, data, data_tipo, ciclo_id,
           valor_centavos, descricao_original, supplier_name_raw, fornecedor_normalizado,
           categoria_provavel, conta_provavel, confianca, extraido_por, estado, movimentacao_id,
           criado_em, decidido_por, contraparte, data_compra, fatura_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pagamento', ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0.9, ?13, ?14, ?15,
           '2026-11-01T10:00:00.000Z', ?16, ?17, ?18, ?19)",
        params![
            r.hash,
            r.external_ref,
            fonte,
            r.tipo,
            r.data,
            ciclo_de(r.data),
            r.valor,
            r.descricao,
            r.supplier_raw,
            r.fornecedor,
            r.categoria,
            r.conta,
            extraido_por,
            estado,
            r.movimentacao_id,
            decidido,
            r.contraparte,
            r.data_compra,
            r.fatura_ref,
        ],
    )
    .expect("inserir registro");
    conn.last_insert_rowid()
}

/// Popula o banco (já com o esquema aplicado). Todo dado é inventado.
pub fn popular(conn: &Connection) {
    // Ciclos de 2025-10 a 2030-12, para o relatório do Bun achar o ciclo de hoje.
    for ano in 2025..=2030 {
        for mes in 1..=12 {
            if ano == 2025 && mes < 10 {
                continue;
            }
            let (ano_fim, mes_fim) = if mes == 12 {
                (ano + 1, 1)
            } else {
                (ano, mes + 1)
            };
            conn.execute(
                "INSERT INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
                params![
                    format!("{ano:04}-{mes:02}"),
                    format!("{ano:04}-{mes:02}-20"),
                    format!("{ano_fim:04}-{mes_fim:02}-19"),
                ],
            )
            .expect("ciclo");
        }
    }

    // Fontes.
    for (id, tipo, descricao, raiz, escopo, motivo) in [
        (
            "disco:e2",
            "disco",
            "Disco E2 (HD externo)",
            Some("/dados/e2"),
            "dentro",
            None,
        ),
        (
            "gmail",
            "email",
            "Caixa de e-mail principal",
            None,
            "dentro",
            None,
        ),
        (
            "onedrive",
            "nuvem",
            "OneDrive pessoal",
            None,
            "dentro",
            None,
        ),
        (
            "conexao:pluggy",
            "banco",
            "Open Finance (Pluggy)",
            None,
            "dentro",
            None,
        ),
        ("celular", "celular", "Celular", None, "dentro", None),
        (
            "whatsapp",
            "mensageiro",
            "Conversas do WhatsApp",
            None,
            "fora",
            Some("de outra pessoa"),
        ),
    ] {
        conn.execute(
            "INSERT INTO fonte (id, tipo, descricao, raiz, escopo, motivo, criada_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-10-01T09:00:00.000Z')",
            params![id, tipo, descricao, raiz, escopo, motivo],
        )
        .expect("fonte");
    }

    // Varreduras: (id, fonte, etapa, responsavel, iniciada, concluida, status, vistos, novos, repetidos, excluidos, erros, resumo)
    type Varredura<'a> = (
        i64,
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        Option<&'a str>,
        &'a str,
        i64,
        i64,
        i64,
        i64,
        i64,
        Option<&'a str>,
    );
    let varreduras: [Varredura<'_>; 6] = [
        (
            1,
            "disco:e2",
            "E1",
            "scan-discos@3",
            "2026-10-02T09:00:00.000Z",
            Some("2026-10-02T11:30:00.000Z"),
            "concluida",
            1200,
            800,
            400,
            150,
            0,
            Some(r#"{"copiados":640}"#),
        ),
        (
            2,
            "gmail",
            "E2",
            "coletar-gmail@2",
            "2026-10-05T08:00:00.000Z",
            Some("2026-10-05T08:40:00.000Z"),
            "concluida",
            300,
            280,
            20,
            0,
            0,
            Some(r#"{"copiados":120}"#),
        ),
        (
            3,
            "conexao:pluggy",
            "E2",
            "importar-pluggy@1",
            "2026-10-06T07:00:00.000Z",
            Some("2026-10-06T07:05:00.000Z"),
            "concluida",
            90,
            90,
            0,
            0,
            0,
            None,
        ),
        (
            4,
            "onedrive",
            "E2",
            "coletar-nuvem@1",
            "2026-10-07T20:00:00.000Z",
            Some("2026-10-07T20:10:00.000Z"),
            "falhou",
            40,
            10,
            0,
            0,
            3,
            None,
        ),
        (
            5,
            "disco:e2",
            "E3",
            "triar@2",
            "2026-10-08T10:00:00.000Z",
            None,
            "rodando",
            50,
            5,
            0,
            0,
            1,
            None,
        ),
        (
            6,
            "gmail",
            "E2",
            "coletar-gmail@2",
            "2026-10-12T08:00:00.000Z",
            Some("2026-10-12T08:20:00.000Z"),
            "interrompida",
            100,
            60,
            0,
            0,
            0,
            Some(r#"{"copiados":30}"#),
        ),
    ];
    for v in varreduras {
        conn.execute(
            "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel, iniciada_em, concluida_em, status,
               vistos, novos, repetidos, excluidos, erros, resumo)
             VALUES (?1, ?2, ?3, 'raiz', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![v.0, v.1, v.2, v.3, v.4, v.5, v.6, v.7, v.8, v.9, v.10, v.11, v.12],
        )
        .expect("varredura");
    }
    conn.execute_batch(
        "INSERT INTO area_excluida (fonte_id, varredura_id, caminho, motivo, arquivos, bytes) VALUES
           ('disco:e2', 1, '/dados/e2/jogos', 'jogos', 300, 5368709120),
           ('disco:e2', 1, '/dados/e2/livros', 'livros', 120, 1073741824),
           ('disco:e2', 1, '/dados/e2/codigo', 'codigo', 900, 214748364);",
    )
    .expect("áreas excluídas");

    // Achados: (n, camada, estado, tipo, protegido, fingerprint, fornecedor, data_chave, data_tipo, valor, caminho_acervo, classificado_por, decidido_por, notas, ext)
    #[allow(clippy::type_complexity)]
    let achados: [(
        u8,
        &str,
        &str,
        Option<&str>,
        Option<i64>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<i64>,
        Option<&str>,
        &str,
        &str,
        Option<&str>,
        &str,
    ); 13] = [
        (
            H_ENERGIA_OUT,
            "B",
            "documento",
            Some("fatura"),
            Some(0),
            Some("fp-energia-out"),
            Some("Energia Exemplo"),
            Some("2026-10-25"),
            Some("vencimento"),
            Some(-18750),
            Some("acervo/01/x.pdf"),
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_ENERGIA_SET,
            "B",
            "documento",
            Some("boleto"),
            Some(0),
            Some("fp-energia-set"),
            Some("Energia Exemplo"),
            Some("2026-09-25"),
            Some("vencimento"),
            Some(-17200),
            Some("acervo/02/x.pdf"),
            "regras@2",
            "usuario",
            None,
            "pdf",
        ),
        (
            H_MERCADO,
            "B",
            "documento",
            Some("comprovante"),
            Some(0),
            Some("fp-mercado"),
            Some("Mercado Delta"),
            Some("2026-10-14"),
            Some("compra"),
            Some(-32000),
            Some("acervo/03/x.pdf"),
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_FATURA_CARTAO,
            "B",
            "documento",
            Some("fatura"),
            Some(1),
            Some("fp-cartao"),
            Some("Banco Alfa"),
            Some("2026-10-10"),
            Some("vencimento"),
            Some(-84500),
            Some("acervo/04/x.pdf"),
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_A_TRIAR_1,
            "B",
            "novo",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_A_TRIAR_2,
            "B",
            "novo",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "regras@2",
            "regras@2",
            None,
            "jpg",
        ),
        (
            H_DESCARTADO,
            "B",
            "descartado",
            Some("outro"),
            Some(0),
            None,
            None,
            None,
            None,
            None,
            None,
            "regras@2",
            "regras@2",
            Some("não é financeiro"),
            "pdf",
        ),
        (
            H_DUVIDA_REGRAS,
            "B",
            "duvida",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "regras@2",
            "regras@2",
            Some("sem texto extraível"),
            "pdf",
        ),
        (
            H_DUVIDA_USUARIO,
            "B",
            "duvida",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "usuario",
            "usuario",
            Some("conferir com a contadora"),
            "png",
        ),
        (
            H_STREAMING_A,
            "B",
            "documento",
            Some("comprovante"),
            Some(0),
            Some("fp-streaming"),
            Some("Streaming Zeta"),
            Some("2026-08-28"),
            Some("pagamento"),
            Some(-3990),
            Some("acervo/0a/x.pdf"),
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_STREAMING_B,
            "B",
            "documento",
            Some("comprovante"),
            Some(0),
            Some("fp-streaming"),
            Some("Streaming Zeta"),
            Some("2026-08-28"),
            Some("pagamento"),
            Some(-3990),
            Some("acervo/0b/x.pdf"),
            "regras@2",
            "regras@2",
            None,
            "pdf",
        ),
        (
            H_EML,
            "B",
            "descartado",
            Some("outro"),
            Some(0),
            None,
            None,
            None,
            None,
            None,
            Some("acervo/0c/x.eml"),
            "regras@2",
            "regras@2",
            Some("propaganda"),
            "eml",
        ),
        (
            H_PASTA_PESSOAL,
            "A",
            "descartado",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "regras@2",
            "regras@2",
            None,
            "jpg",
        ),
    ];
    for a in achados {
        let ciclo = a.7.map(ciclo_de);
        conn.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada, caminho_acervo, estado,
               document_type, protegido_senha, canonical_fingerprint, supplier_name_raw, data_chave, data_chave_tipo,
               ciclo_id, valor_centavos, confianca, classificado_por, primeiro_visto_em, notas, decidido_por)
             VALUES (?1, 2048, 'application/pdf', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0.92, ?14,
               '2026-10-03T12:00:00.000Z', ?15, ?16)",
            params![hash(a.0), a.14, a.1, a.10, a.2, a.3, a.4, a.5, a.6, a.7, a.8, ciclo, a.9, a.11, a.13, a.12],
        )
        .expect("achado");
    }
    conn.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', 'Fatura de energia Exemplo. Vencimento 25/10/2026. Total R$ 187,50')",
        [hash(H_ENERGIA_OUT)],
    )
    .expect("texto");

    // Origens: (hash, fonte, varredura, tipo, chave, caminho, mtime, filename, gmail msg, from, assunto, data, mensagem_ref)
    #[allow(clippy::type_complexity)]
    let origens: [(
        u8,
        &str,
        i64,
        &str,
        &str,
        Option<&str>,
        Option<&str>,
        &str,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
    ); 17] = [
        (
            H_ENERGIA_OUT,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/contas/energia-out.pdf:1760000000000",
            Some("/dados/e2/contas/energia-out.pdf"),
            Some("2026-10-21T10:00:00Z"),
            "energia-out.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_ENERGIA_OUT,
            "gmail",
            2,
            "gmail",
            "gmail:m100:h1",
            None,
            None,
            "fatura-energia.pdf",
            Some("m100"),
            Some("Energia Exemplo <cobranca@energia-exemplo.com.br>"),
            Some("Sua fatura de outubro"),
            Some("2026-10-21T08:15:00Z"),
            Some("anexo"),
        ),
        (
            H_ENERGIA_SET,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/contas/energia-set.pdf:1757000000000",
            Some("/dados/e2/contas/energia-set.pdf"),
            Some("2026-09-21T10:00:00Z"),
            "energia-set.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_ENERGIA_SET,
            "onedrive",
            4,
            "onedrive",
            "onedrive:contas/energia-set.pdf",
            Some("onedrive:contas/energia-set.pdf"),
            Some("2026-09-22T10:00:00Z"),
            "energia-set.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_MERCADO,
            "gmail",
            2,
            "gmail",
            "gmail:m101:h3",
            None,
            None,
            "comprovante.pdf",
            Some("m101"),
            Some("Mercado Delta <nf@mercado-delta.com.br>"),
            Some("Comprovante da sua compra"),
            Some("2026-10-14T19:00:00Z"),
            Some("anexo"),
        ),
        (
            H_FATURA_CARTAO,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/cartao/fatura-out.pdf:1760100000000",
            Some("/dados/e2/cartao/fatura-out.pdf"),
            Some("2026-10-10T09:00:00Z"),
            "fatura-out.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_A_TRIAR_1,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/misc/a1.pdf:1750000000000",
            Some("/dados/e2/misc/a1.pdf"),
            Some("2026-06-01T10:00:00Z"),
            "a1.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_A_TRIAR_2,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/misc/a2.jpg:1750000100000",
            Some("/dados/e2/misc/a2.jpg"),
            Some("2026-06-02T10:00:00Z"),
            "a2.jpg",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_DESCARTADO,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/misc/manual.pdf:1740000000000",
            Some("/dados/e2/misc/manual.pdf"),
            Some("2026-03-02T10:00:00Z"),
            "manual.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_DUVIDA_REGRAS,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/misc/scan1.pdf:1750000200000",
            Some("/dados/e2/misc/scan1.pdf"),
            Some("2026-06-03T10:00:00Z"),
            "scan1.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_DUVIDA_USUARIO,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/misc/foto.png:1750000300000",
            Some("/dados/e2/misc/foto.png"),
            Some("2026-06-04T10:00:00Z"),
            "foto.png",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_STREAMING_A,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/contas/streaming-a.pdf:1756000000000",
            Some("/dados/e2/contas/streaming-a.pdf"),
            Some("2026-08-28T10:00:00Z"),
            "streaming-a.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_STREAMING_B,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/contas/streaming-b.pdf:1756000100000",
            Some("/dados/e2/contas/streaming-b.pdf"),
            Some("2026-08-28T10:05:00Z"),
            "streaming-b.pdf",
            None,
            None,
            None,
            None,
            None,
        ),
        (
            H_EML,
            "gmail",
            2,
            "gmail",
            "gmail:m100:eml",
            None,
            None,
            "Sua fatura de outubro.eml",
            Some("m100"),
            Some("Energia Exemplo <cobranca@energia-exemplo.com.br>"),
            Some("Sua fatura de outubro"),
            Some("2026-10-21T08:15:00Z"),
            Some("eml"),
        ),
        (
            H_EML,
            "gmail",
            2,
            "gmail",
            "gmail:m101:eml",
            None,
            None,
            "Comprovante da sua compra.eml",
            Some("m101"),
            Some("Mercado Delta <nf@mercado-delta.com.br>"),
            Some("Comprovante da sua compra"),
            Some("2026-10-14T19:00:00Z"),
            Some("eml"),
        ),
        (
            H_EML,
            "gmail",
            6,
            "gmail",
            "gmail:m102:eml",
            None,
            None,
            "Promoção da semana.eml",
            Some("m102"),
            Some("Loja Promo <oi@loja-promo.com.br>"),
            Some("Promoção da semana"),
            Some("2026-10-11T09:00:00Z"),
            Some("eml"),
        ),
        (
            H_PASTA_PESSOAL,
            "disco:e2",
            1,
            "local_file",
            "local:/dados/e2/pessoal/ferias/f1.jpg:1700000000000",
            Some("/dados/e2/pessoal/ferias/f1.jpg"),
            Some("2026-01-02T10:00:00Z"),
            "f1.jpg",
            None,
            None,
            None,
            None,
            None,
        ),
    ];
    for o in origens {
        conn.execute(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath, local_mtime,
               filename, file_size_bytes, gmail_message_id, gmail_from, gmail_subject, gmail_date, mensagem_ref, visto_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 2048, ?9, ?10, ?11, ?12, ?13, '2026-10-03T12:00:00.000Z')",
            params![hash(o.0), o.1, o.2, o.3, o.4, o.5, o.6, o.7, o.8, o.9, o.10, o.11, o.12],
        )
        .expect("origem");
    }
    // Relação entre achados.
    conn.execute(
        "INSERT INTO relacao (a, b, tipo, confianca, motivo) VALUES (?1, ?2, 'duplicidade', 0.8, 'mesmo texto normalizado')",
        params![hash(H_STREAMING_A), hash(H_STREAMING_B)],
    )
    .expect("relação");

    // Registros dos documentos.
    let r_energia_out = inserir_registro(
        conn,
        &Reg {
            hash: Some(hash(H_ENERGIA_OUT)),
            fonte: "disco:e2",
            tipo: "pagamento",
            data: "2026-10-25",
            valor: -18750,
            descricao: "Fatura de energia",
            supplier_raw: Some("Energia Exemplo"),
            fornecedor: Some("Energia Exemplo"),
            categoria: Some("Energia"),
            conta: Some("Conta Corrente Alfa"),
            extraido_por: "extrator@1",
            estado: "conferido",
            ..Reg::default()
        },
    );
    let r_energia_set = inserir_registro(
        conn,
        &Reg {
            hash: Some(hash(H_ENERGIA_SET)),
            fonte: "disco:e2",
            tipo: "pagamento",
            data: "2026-09-25",
            valor: -17200,
            descricao: "Boleto de energia",
            supplier_raw: Some("Energia Exemplo"),
            fornecedor: Some("Energia Exemplo"),
            categoria: Some("Energia"),
            conta: Some("Conta Corrente Alfa"),
            extraido_por: "extrator@1",
            estado: "promovido",
            movimentacao_id: Some("mov-101"),
            decidido_por: Some("usuario"),
            ..Reg::default()
        },
    );
    let r_mercado = inserir_registro(
        conn,
        &Reg {
            hash: Some(hash(H_MERCADO)),
            fonte: "gmail",
            tipo: "compra",
            data: "2026-10-14",
            valor: -32000,
            descricao: "Compra no Mercado Delta",
            supplier_raw: Some("Mercado Delta"),
            fornecedor: Some("Mercado Delta"),
            categoria: Some("Alimentação"),
            conta: Some("Cartão Beta"),
            extraido_por: "extrator@1",
            estado: "aprovado",
            decidido_por: Some("usuario"),
            ..Reg::default()
        },
    );

    // Registros do Open Finance, ciclo a ciclo: (data, tipo, valor, descrição, fornecedor, categoria, conta, contraparte)
    #[allow(clippy::type_complexity)]
    let pluggy: [(
        &str,
        &str,
        i64,
        &str,
        Option<&str>,
        Option<&str>,
        &str,
        Option<&str>,
    ); 22] = [
        (
            "2026-08-05",
            "salario",
            520000,
            "Salário agosto",
            Some("Empresa Empregadora Exemplo"),
            Some("Salário"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-09-05",
            "salario",
            520000,
            "Salário setembro",
            Some("Empresa Empregadora Exemplo"),
            Some("Salário"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-10-05",
            "salario",
            520000,
            "Salário outubro",
            Some("Empresa Empregadora Exemplo"),
            Some("Salário"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-08-25",
            "pagamento",
            -16500,
            "Energia agosto",
            Some("Energia Exemplo"),
            Some("Energia"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-09-25",
            "pagamento",
            -17200,
            "Energia setembro",
            Some("Energia Exemplo"),
            Some("Energia"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-10-25",
            "pagamento",
            -18750,
            "Energia outubro",
            Some("Energia Exemplo"),
            Some("Energia"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-08-22",
            "compra",
            -28000,
            "Mercado Delta",
            Some("Mercado Delta"),
            Some("Alimentação"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-09-22",
            "compra",
            -30500,
            "Mercado Delta",
            Some("Mercado Delta"),
            Some("Alimentação"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-10-22",
            "compra",
            -32000,
            "Mercado Delta",
            Some("Mercado Delta"),
            Some("Alimentação"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-08-28",
            "compra",
            -3990,
            "Streaming Zeta",
            Some("Streaming Zeta"),
            Some("Assinaturas"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-09-28",
            "compra",
            -3990,
            "Streaming Zeta",
            Some("Streaming Zeta"),
            Some("Assinaturas"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-10-28",
            "compra",
            -3990,
            "Streaming Zeta",
            Some("Streaming Zeta"),
            Some("Assinaturas"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-09-10",
            "debito",
            -4500,
            "Pix Padaria Épsilon",
            None,
            None,
            "Conta Corrente Alfa",
            Some("Padaria Épsilon"),
        ),
        (
            "2026-10-10",
            "debito",
            -5200,
            "Pix Padaria Épsilon",
            None,
            None,
            "Conta Corrente Alfa",
            Some("Padaria Épsilon"),
        ),
        (
            "2026-10-12",
            "debito",
            -9900,
            "Compra sem nome",
            None,
            Some("Sem categoria"),
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-09-30",
            "transferencia_interna",
            -50000,
            "Para a reserva",
            Some("Conta Reserva Gama"),
            None,
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-09-30",
            "transferencia_interna",
            50000,
            "Da conta corrente",
            Some("Conta Corrente Alfa"),
            None,
            "Conta Reserva Gama",
            None,
        ),
        (
            "2026-10-15",
            "investimento",
            -100000,
            "Aplicação CDB",
            Some("Corretora Ômega"),
            None,
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-10-10",
            "pagamento_fatura",
            -120000,
            "Pagamento da fatura do cartão",
            Some("Banco Alfa"),
            None,
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-09-10",
            "pagamento_fatura",
            -95000,
            "Pagamento da fatura do cartão",
            Some("Banco Alfa"),
            None,
            "Conta Corrente Alfa",
            None,
        ),
        (
            "2026-10-18",
            "estorno",
            3150,
            "Estorno Mercado Delta",
            Some("Mercado Delta"),
            Some("Alimentação"),
            "Cartão Beta",
            None,
        ),
        (
            "2026-10-26",
            "tarifa",
            -2990,
            "Tarifa mensal",
            Some("Banco Alfa"),
            Some("Tarifas"),
            "Conta Corrente Alfa",
            None,
        ),
    ];
    let mut id_estorno = 0;
    for (data, tipo, valor, desc, forn, cat, conta, contra) in pluggy {
        let fatura = if conta == "Cartão Beta" && valor < 0 {
            Some(if data < "2026-10-10" {
                "b-set"
            } else {
                "b-out"
            })
        } else {
            None
        };
        let id = inserir_registro(
            conn,
            &Reg {
                fonte: "conexao:pluggy",
                tipo,
                data,
                valor,
                descricao: desc,
                fornecedor: forn,
                categoria: cat,
                conta: Some(conta),
                contraparte: contra,
                fatura_ref: fatura,
                data_compra: if tipo == "compra" { Some(data) } else { None },
                ..Reg::default()
            },
        );
        if tipo == "estorno" {
            id_estorno = id;
        }
    }
    // Faturas do cartão: a fatura é um registro `fatura_cartao` cujo valor é o total com sinal
    // trocado. A de setembro fecha com as compras ligadas (66.480); a de outubro não fecha:
    // diz 84.500 e as compras ligadas somam 35.990.
    for (externo, vencimento, total) in [
        (
            "pluggy:bill:b-set",
            "2026-09-10",
            28000 + 30500 + 3990 + 3990,
        ),
        ("pluggy:bill:b-out", "2026-10-10", 84500),
    ] {
        inserir_registro(
            conn,
            &Reg {
                external_ref: Some(externo),
                fonte: "conexao:pluggy",
                tipo: "fatura_cartao",
                data: vencimento,
                valor: -total,
                descricao: "Fatura do cartão",
                conta: Some("Cartão Beta"),
                ..Reg::default()
            },
        );
    }

    // Conciliação: o registro do documento e o da conexão descrevem a conta de outubro.
    conn.execute(
        "INSERT INTO relacao_registro (a, b, tipo, confianca, motivo)
         SELECT 1, r.id, 'conciliacao', 0.95, 'mesmo valor e data' FROM registro r
          WHERE r.fonte_id = 'conexao:pluggy' AND r.valor_centavos = -18750 AND r.data = '2026-10-25'",
        [],
    )
    .expect("conciliação");

    // Preenchimento para a conta de uso regular (conta_buracos): 24 registros em 3 meses, sem setembro,
    // e 22 na reserva, só em julho e outubro.
    for (mes, dias) in [("2026-07", 8), ("2026-08", 8), ("2026-10", 8)] {
        for d in 0..dias {
            let data = format!("{mes}-{:02}", 2 + d);
            inserir_registro(
                conn,
                &Reg {
                    fonte: "conexao:pluggy",
                    tipo: "debito",
                    data: &data,
                    valor: -(1000 + d * 10),
                    descricao: "Compra miúda",
                    fornecedor: Some("Padaria Épsilon"),
                    categoria: Some("Alimentação"),
                    conta: Some("Conta de Uso Regular"),
                    ..Reg::default()
                },
            );
        }
    }
    for (mes, dias) in [("2026-07", 11), ("2026-10", 11)] {
        for d in 0..dias {
            let data = format!("{mes}-{:02}", 2 + d);
            inserir_registro(
                conn,
                &Reg {
                    fonte: "conexao:pluggy",
                    tipo: "credito",
                    data: &data,
                    valor: 500 + d * 10,
                    descricao: "Rendimento",
                    fornecedor: Some("Corretora Ômega"),
                    categoria: Some("Rendimentos"),
                    conta: Some("Conta Reserva Gama"),
                    ..Reg::default()
                },
            );
        }
    }

    // Lotes: o 1 aprovado e promovido (tem o registro promovido), o 2 aberto.
    conn.execute_batch(&format!(
        "INSERT INTO lote (id, nome, criado_em, aprovado_em, promovido_em) VALUES
           (1, 'Lote de setembro', '2026-10-20T10:00:00.000Z', '2026-10-21T10:00:00.000Z', '2026-10-22T10:00:00.000Z'),
           (2, 'Lote de outubro', '2026-11-02T10:00:00.000Z', NULL, NULL);
         INSERT INTO lote_item (lote_id, alvo_tipo, alvo_id) VALUES
           (1, 'achado', '{h2}'), (1, 'registro', '{r2}'),
           (2, 'achado', '{h1}'), (2, 'achado', '{h3}'), (2, 'registro', '{r1}'), (2, 'registro', '{r3}');",
        h2 = hash(H_ENERGIA_SET),
        h1 = hash(H_ENERGIA_OUT),
        h3 = hash(H_MERCADO),
        r2 = r_energia_set,
        r1 = r_energia_out,
        r3 = r_mercado,
    ))
    .expect("lotes");

    // Decisões: (id, tipo, alvo, campo, antes, depois, autor, motivo, em, desfaz)
    #[allow(clippy::type_complexity)]
    let decisoes: Vec<(
        i64,
        &str,
        String,
        &str,
        Option<&str>,
        Option<&str>,
        &str,
        Option<&str>,
        &str,
        Option<i64>,
    )> = vec![
        (
            1,
            "achado",
            hash(H_ENERGIA_OUT),
            "estado",
            Some("\"novo\""),
            Some("\"documento\""),
            "regras@2",
            None,
            "2026-10-08T10:00:00.000Z",
            None,
        ),
        (
            2,
            "achado",
            hash(H_ENERGIA_SET),
            "estado",
            Some("\"novo\""),
            Some("\"descartado\""),
            "regras@2",
            None,
            "2026-10-08T10:00:01.000Z",
            None,
        ),
        (
            3,
            "achado",
            hash(H_ENERGIA_SET),
            "estado",
            Some("\"descartado\""),
            Some("\"documento\""),
            "usuario",
            Some("era a conta de luz"),
            "2026-10-15T09:00:00.000Z",
            None,
        ),
        (
            4,
            "achado",
            hash(H_DUVIDA_USUARIO),
            "estado",
            Some("\"novo\""),
            Some("\"duvida\""),
            "usuario",
            None,
            "2026-10-16T09:00:00.000Z",
            None,
        ),
        (
            5,
            "achado",
            hash(H_DUVIDA_USUARIO),
            "estado",
            Some("\"duvida\""),
            Some("\"documento\""),
            "usuario",
            None,
            "2026-10-16T09:05:00.000Z",
            None,
        ),
        (
            6,
            "achado",
            hash(H_DUVIDA_USUARIO),
            "estado",
            Some("\"documento\""),
            Some("\"duvida\""),
            "usuario",
            Some("desfeita"),
            "2026-10-16T09:06:00.000Z",
            Some(5),
        ),
        (
            7,
            "registro",
            r_energia_set.to_string(),
            "estado",
            Some("\"conferido\""),
            Some("\"aprovado\""),
            "usuario",
            None,
            "2026-10-20T09:00:00.000Z",
            None,
        ),
        (
            8,
            "fornecedor",
            "Streaming Zeta".to_owned(),
            "recorrente",
            Some("false"),
            Some("true"),
            "usuario",
            None,
            "2026-10-17T09:00:00.000Z",
            None,
        ),
        (
            9,
            "lote",
            "1".to_owned(),
            "aprovado_em",
            None,
            Some("\"2026-10-21T10:00:00.000Z\""),
            "usuario",
            None,
            "2026-10-21T10:00:00.000Z",
            None,
        ),
        (
            10,
            "registro",
            id_estorno.to_string(),
            "categoria",
            Some("null"),
            Some("\"Alimentação\""),
            "regras@2",
            None,
            "2026-10-19T09:00:00.000Z",
            None,
        ),
    ];
    for d in decisoes {
        conn.execute(
            "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![d.0, d.1, d.2, d.3, d.4, d.5, d.6, d.7, d.8, d.9],
        )
        .expect("decisão");
    }

    // Regras de senha e tentativas (nunca a senha).
    conn.execute_batch(&format!(
        "INSERT INTO regra_senha (id, fornecedor, document_type, estrategia, prioridade, ativa, sucessos, falhas, ultimo_sucesso_em)
           VALUES (1, 'Banco Alfa', 'fatura', 'cpf_primeiros:5', 10, 1, 3, 1, '2026-10-10T10:00:00.000Z'),
                  (2, 'Cartão Beta', '*', 'senha_propria:cartao-beta', 20, 1, 0, 0, NULL);
         INSERT INTO tentativa_senha (id, content_hash, estrategia, sucesso, em) VALUES
           (1, '{h4}', 'cpf_completo', 0, '2026-10-10T09:59:00.000Z'),
           (2, '{h4}', 'cpf_primeiros:5', 1, '2026-10-10T10:00:00.000Z');",
        h4 = hash(H_FATURA_CARTAO),
    ))
    .expect("regras de senha");

    // Pendências: duas abertas (uma sobre o achado, uma sobre o registro) e uma respondida.
    conn.execute(
        "INSERT INTO pendencia (id, etapa, tipo, referencia, pergunta, criada_em, resolvida_em, resposta) VALUES
           (1, 'E3', 'pdf_protegido', ?1, 'PDF protegido sem regra', '2026-10-10T10:05:00.000Z', NULL, NULL),
           (2, 'E4', 'valor_divergente', ?2, 'O valor do registro não bate com o documento?', '2026-10-18T10:00:00.000Z', NULL, NULL),
           (3, 'E3', 'fornecedor', ?3, 'Quem é o fornecedor deste achado?', '2026-10-05T10:00:00.000Z', '2026-10-06T10:00:00.000Z', 'Mercado Delta')",
        params![
            format!("achado:{}", hash(H_FATURA_CARTAO)),
            format!("registro:{r_mercado}"),
            format!("achado:{}", hash(H_MERCADO)),
        ],
    )
    .expect("pendências");
}

/// Uma fixture grande e sintética para medir desempenho (INV-16): `n_achados` achados,
/// três origens a cada dois achados e dois registros por documento.
pub fn popular_grande(conn: &Connection, n_achados: u32) {
    conn.execute_batch("BEGIN").expect("transação");
    conn.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:g', 'disco', 'Disco grande')",
        [],
    )
    .expect("fonte");
    conn.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel) VALUES (100, 'disco:g', 'E1', 'raiz', 'scan@1')",
        [],
    )
    .expect("varredura");
    for ano in 2024..=2027 {
        for mes in 1..=12 {
            let (ano_fim, mes_fim) = if mes == 12 {
                (ano + 1, 1)
            } else {
                (ano, mes + 1)
            };
            conn.execute(
                "INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
                params![
                    format!("{ano:04}-{mes:02}"),
                    format!("{ano:04}-{mes:02}-20"),
                    format!("{ano_fim:04}-{mes_fim:02}-19"),
                ],
            )
            .expect("ciclo");
        }
    }
    let estados = ["novo", "documento", "documento", "descartado", "duvida"];
    for i in 0..n_achados {
        let h = format!("{i:064x}");
        let estado = estados[(i % 5) as usize];
        let mes = 10 + (i % 12);
        let (ano, mes) = if mes > 12 {
            (2026, mes - 12)
        } else {
            (2025, mes)
        };
        let data = format!("{ano:04}-{mes:02}-{:02}", 1 + (i % 28));
        let forn = format!("Fornecedor sintético {}", i % 400);
        conn.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, document_type, supplier_name_raw,
               data_chave, ciclo_id, valor_centavos, classificado_por, decidido_por, caminho_acervo, notas)
             VALUES (?1, 1000, 'B', ?2, 'fatura', ?3, ?4, ?5, ?6, 'regras@2', 'regras@2', ?7, 'nota')",
            params![h, estado, forn, data, ciclo_de(&data), -(1000 + i64::from(i % 5000)), if i % 3 == 0 { Some("acervo/x") } else { None }],
        )
        .expect("achado grande");
        conn.execute(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath, local_mtime, filename)
             VALUES (?1, 'disco:g', 100, 'local_file', ?2, ?3, ?4, ?5)",
            params![h, format!("local:{i}"), format!("/g/p{}/a{i}.pdf", i % 50), format!("{data}T10:00:00Z"), format!("a{i}.pdf")],
        )
        .expect("origem grande");
        if i % 2 == 0 {
            conn.execute(
                "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, filename, gmail_from, gmail_subject, gmail_date, gmail_message_id, mensagem_ref)
                 VALUES (?1, 'disco:g', 100, 'gmail', ?2, ?3, ?4, 'Assunto', ?5, ?6, 'eml')",
                params![h, format!("gmail:{i}"), format!("m{i}.eml"), format!("Fulano <f@dominio{}.com>", i % 300), format!("{data}T10:00:00Z"), format!("msg{i}")],
            )
            .expect("origem e-mail grande");
        }
        for k in 0..2 {
            conn.execute(
                "INSERT INTO registro (content_hash, fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos,
                   descricao_original, fornecedor_normalizado, categoria_provavel, conta_provavel, extraido_por, decidido_por)
                 VALUES (?1, 'disco:g', ?2, ?3, 'pagamento', ?4, ?5, 'Registro sintético', ?6, 'Categoria', 'Conta G', 'extrator@1', 'extrator@1')",
                params![h, if (i + k) % 11 == 0 { "pagamento_fatura" } else { "pagamento" }, data, ciclo_de(&data), -(500 + i64::from(i % 900)), forn],
            )
            .expect("registro grande");
        }
    }
    conn.execute_batch("COMMIT").expect("commit");
}

/// A ordem de grandeza do inventário real (medida em 2026-11-13): 300 mil achados, 600 mil
/// origens, 57 mil e-mails de cerca de 1.000 remetentes, 4 mil registros. Só números: nenhum
/// dado real. Leva alguns segundos.
pub fn popular_escala_real(conn: &Connection) {
    popular_escala(conn, 300_000, 600_000, 57_000, 1_000);
}

/// Como [`popular_escala_real`], com os tamanhos à escolha.
pub fn popular_escala(conn: &Connection, achados: u32, origens: u32, emails: u32, remetentes: u32) {
    conn.execute_batch("BEGIN").expect("transação");
    for (id, tipo) in [
        ("disco:g", "disco"),
        ("disco:h", "disco"),
        ("gmail", "email"),
        ("conexao:pluggy", "banco"),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao) VALUES (?1, ?2, ?3)",
            params![id, tipo, format!("Fonte {id}")],
        )
        .expect("fonte");
    }
    for (id, fonte) in [(100, "disco:g"), (101, "disco:h"), (102, "gmail")] {
        conn.execute(
            "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel, status, vistos, novos) VALUES (?1, ?2, 'E1', 'raiz', 'scan@1', 'concluida', 10, 10)",
            params![id, fonte],
        )
        .expect("varredura");
    }
    for ano in 2024..=2027 {
        for mes in 1..=12 {
            let (ano_fim, mes_fim) = if mes == 12 {
                (ano + 1, 1)
            } else {
                (ano, mes + 1)
            };
            conn.execute(
                "INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
                params![
                    format!("{ano:04}-{mes:02}"),
                    format!("{ano:04}-{mes:02}-20"),
                    format!("{ano_fim:04}-{mes_fim:02}-19")
                ],
            )
            .expect("ciclo");
        }
    }
    let mut achado = conn
        .prepare(
            "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, document_type, supplier_name_raw,
               data_chave, ciclo_id, valor_centavos, classificado_por, decidido_por, caminho_acervo, notas, extensao)
             VALUES (?1, 1000, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'regras@2', 'regras@2', ?9, 'nota', 'pdf')",
        )
        .expect("preparar achado");
    let mut origem = conn
        .prepare(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key, local_filepath, local_mtime,
               filename, file_size_bytes, gmail_message_id, gmail_from, gmail_subject, gmail_date, mensagem_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 100, ?9, ?10, ?11, ?12, ?13)",
        )
        .expect("preparar origem");
    let data_de = |i: u32| {
        let mes = 1 + (i / 28) % 12;
        format!("2026-{mes:02}-{:02}", 1 + i % 28)
    };
    let mut n_origens = 0u32;
    for i in 0..achados {
        let h = format!("{i:064x}");
        let estado = if i % 25 == 0 {
            "documento"
        } else if i % 8 == 0 {
            "duvida"
        } else if i % 97 == 0 {
            "novo"
        } else {
            "descartado"
        };
        let data = data_de(i);
        let doc = estado == "documento";
        achado
            .execute(params![
                h,
                if i % 3 == 0 { "A" } else { "B" },
                estado,
                doc.then_some("fatura"),
                doc.then(|| format!("Fornecedor {}", i % 400)),
                doc.then_some(data.clone()),
                doc.then(|| ciclo_de(&data)),
                doc.then_some(-(1000 + i64::from(i % 5000))),
                (i % 4 == 0).then_some("acervo/x"),
            ])
            .expect("achado");
        // A origem local.
        origem
            .execute(params![
                h,
                "disco:g",
                100,
                "local_file",
                format!("local:{i}"),
                format!("{}/{}", pasta_profunda((i / 3) % 81_000), nome_longo(i)),
                format!("{data}T10:00:00Z"),
                nome_longo(i),
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
            ])
            .expect("origem");
        n_origens += 1;
    }
    // E-mails: `emails` mensagens de `remetentes` remetentes, em achados espalhados.
    let passo = (achados / emails.max(1)).max(1);
    for k in 0..emails {
        let i = (k * passo) % achados;
        let h = format!("{i:064x}");
        let r = k % remetentes.max(1);
        origem
            .execute(params![
                h,
                "gmail",
                102,
                "gmail",
                format!("gmail:{k}:eml"),
                Option::<String>::None,
                Option::<String>::None,
                format!("mensagem {k}.eml"),
                format!("msg{k}"),
                format!("Remetente {r} <u{r}@dominio{r}.com.br>"),
                format!("Assunto {k}"),
                format!("{}T08:00:00Z", data_de(k)),
                "eml",
            ])
            .expect("e-mail");
        n_origens += 1;
    }
    // O resto das origens: o mesmo conteúdo achado em outro disco.
    let mut k = 0u32;
    while n_origens < origens {
        let i = (k * 7) % achados;
        let h = format!("{i:064x}");
        let data = data_de(i + 3);
        origem
            .execute(params![
                h,
                "disco:h",
                101,
                "local_file",
                format!("local:h:{k}"),
                format!("/h/q{}/b{k}.pdf", k % 5_000),
                format!("{data}T11:00:00Z"),
                format!("b{k}.pdf"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
            ])
            .expect("origem extra");
        n_origens += 1;
        k += 1;
    }
    drop(achado);
    drop(origem);
    for j in 0..4_000u32 {
        let data = data_de(j);
        conn.execute(
            "INSERT INTO registro (fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos, descricao_original,
               fornecedor_normalizado, categoria_provavel, conta_provavel, extraido_por, decidido_por)
             VALUES ('conexao:pluggy', ?1, ?2, 'pagamento', ?3, ?4, 'Registro', ?5, 'Categoria', 'Conta G', 'pluggy@1', 'pluggy@1')",
            params![if j % 17 == 0 { "pagamento_fatura" } else { "compra" }, data, ciclo_de(&data), -(500 + i64::from(j % 900)), format!("Fornecedor {}", j % 400)],
        )
        .expect("registro");
    }
    conn.execute_batch("COMMIT").expect("commit");
}

/// Uma pasta de 6 a 12 níveis com nomes longos, como as de um disco pessoal de verdade.
pub fn pasta_profunda(k: u32) -> String {
    let niveis = 6 + (k % 7) as usize;
    let mut caminho = String::from("/mnt/d/Pessoal/Arquivos antigos do computador");
    for n in 0..niveis {
        // O último nível é único por pasta: com `k` até 81 mil, são 81 mil pastas distintas.
        let parte = if n + 1 == niveis {
            k
        } else {
            (k / 3u32.pow(n as u32 % 6)) % 11
        };
        caminho.push_str(&format!(
            "/Pasta numero {n} com nome comprido {parte} e mais um pouco"
        ));
    }
    caminho
}

/// Um nome de arquivo longo e único.
pub fn nome_longo(i: u32) -> String {
    format!("Documento escaneado da familia com nome bem comprido numero {i}.pdf")
}
