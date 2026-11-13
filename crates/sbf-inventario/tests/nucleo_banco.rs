//! Abrir, planejar e migrar o inventário (INV-01, INV-02) e o log só de acréscimo (INV-07).
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use nucleo_comum::*;
use rusqlite::Connection;
use sbf_inventario::banco::{ErroDoBanco, EstadoDoBanco, Inventario};
use sbf_inventario::{Acao, Comandos, PedidoDeDecisao};

fn bytes(caminho: &std::path::Path) -> Vec<u8> {
    std::fs::read(caminho).unwrap()
}

fn colunas_de_todas_as_tabelas(conexao: &Connection) -> Vec<String> {
    let mut consulta = conexao
        .prepare(
            "SELECT m.name || '.' || c.name FROM sqlite_master m, pragma_table_info(m.name) c
              WHERE m.type = 'table' ORDER BY 1",
        )
        .unwrap();
    consulta
        .query_map([], |l| l.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn marcar_documento(alvo: sbf_inventario::Ref) -> PedidoDeDecisao {
    PedidoDeDecisao {
        acao: Acao::MarcarDocumento,
        alvos: vec![alvo],
        motivo: None,
    }
}

#[test]
fn inv01_abrir_e_ver_o_plano_nao_mudam_o_banco() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    let antes = bytes(&caminho);

    let inventario = Inventario::abrir(&caminho).unwrap();
    assert_eq!(inventario.estado(), EstadoDoBanco::DoBun);
    let plano = inventario.plano().unwrap();
    assert!(plano.precisa_migrar());
    assert!(plano.faz_copia);
    assert_eq!(plano.versao_atual, 0);
    assert_eq!(plano.versao_final, 2);
    let versoes: Vec<u32> = plano.passos.iter().map(|p| p.versao).collect();
    assert_eq!(versoes, vec![1, 2]);
    assert!(plano.passos[0].descricao.contains("adota o banco do Bun"));
    assert_eq!(inventario.caminho(), caminho.as_path());
    drop(inventario);

    assert_eq!(bytes(&caminho), antes, "abrir e planejar mudaram o arquivo");
    let conexao = Connection::open(&caminho).unwrap();
    assert_eq!(valor(&conexao, "PRAGMA user_version"), "0");
    drop(conexao);

    // Arquivo de 0 bytes: continua com 0 bytes, sem `-wal` nem `-shm` ao lado.
    let vazio = pasta.path().join("vazio.sqlite");
    std::fs::write(&vazio, b"").unwrap();
    let inventario = Inventario::abrir(&vazio).unwrap();
    assert_eq!(inventario.estado(), EstadoDoBanco::Vazio);
    assert!(inventario.plano().unwrap().precisa_migrar());
    drop(inventario);
    assert_eq!(
        std::fs::metadata(&vazio).unwrap().len(),
        0,
        "abrir gravou no arquivo vazio"
    );
    assert_eq!(bytes(&vazio), Vec::<u8>::new());
    for sufixo in ["-wal", "-shm", "-journal"] {
        let mut lado = vazio.as_os_str().to_owned();
        lado.push(sufixo);
        assert!(
            !std::path::Path::new(&lado).exists(),
            "abrir criou {sufixo}"
        );
    }

    // Arquivo que não existe: continua não existindo.
    let ausente = pasta.path().join("ausente.sqlite");
    let inventario = Inventario::abrir(&ausente).unwrap();
    assert_eq!(inventario.estado(), EstadoDoBanco::Vazio);
    assert!(inventario.plano().unwrap().precisa_migrar());
    drop(inventario);
    assert!(!ausente.exists(), "abrir criou o arquivo");
}

#[test]
fn inv01_nenhuma_decisao_antes_de_migrar() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    let antes = bytes(&caminho);
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    let erro = inventario
        .executar(&marcar_documento(achado(1)))
        .unwrap_err();
    assert!(erro.to_string().contains("migrado"), "{erro}");
    drop(inventario);
    assert_eq!(bytes(&caminho), antes);
}

#[test]
fn inv01_migrar_faz_a_copia_antes() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    let original = bytes(&caminho);
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    let copia = pasta.path().join("antes.sqlite");

    let feita = inventario.migrar(&copia).unwrap();
    assert_eq!(feita.copia.as_deref(), Some(copia.as_path()));
    assert!(feita.adotou_o_bun);
    assert_eq!((feita.versao_inicial, feita.versao_final), (0, 2));

    // A cópia é o banco de antes: versão 0 do Bun, sem o log, com os mesmos dados.
    let c = Connection::open(&copia).unwrap();
    assert_eq!(valor(&c, "PRAGMA user_version"), "0");
    assert_eq!(
        valor(&c, "SELECT valor FROM meta WHERE chave = 'schema_versao'"),
        "2"
    );
    assert_eq!(
        valor(
            &c,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'decisao'"
        ),
        "0"
    );
    assert_eq!(valor(&c, "SELECT COUNT(*) FROM achado"), "4");
    std::fs::write(pasta.path().join("original.sqlite"), &original).unwrap();
    let orig = Connection::open(pasta.path().join("original.sqlite")).unwrap();
    assert_eq!(retrato_base(&orig), retrato_base(&c));

    // Uma segunda migração não tem o que fazer e não copia.
    let de_novo = inventario
        .migrar(pasta.path().join("outra.sqlite"))
        .unwrap();
    assert_eq!(de_novo.copia, None);
    assert!(!pasta.path().join("outra.sqlite").exists());
}

/// O retrato das tabelas da base (sem as colunas que a migração 2 acrescenta).
fn retrato_base(conexao: &Connection) -> String {
    let mut saida = String::new();
    for tabela in [
        "achado",
        "registro",
        "origem",
        "pendencia",
        "relacao",
        "fonte",
    ] {
        let mut consulta = conexao
            .prepare(&format!("SELECT * FROM {tabela} ORDER BY 1"))
            .unwrap();
        let n = consulta.column_count();
        let mut linhas = consulta.query([]).unwrap();
        while let Some(l) = linhas.next().unwrap() {
            for i in 0..n {
                saida.push_str(&format!("{:?}|", l.get_ref(i).unwrap()));
            }
            saida.push('\n');
        }
    }
    saida
}

#[test]
fn inv01_depois_de_migrar_abrir_de_novo_nao_muda_nada() {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    let caminho = inventario.caminho().to_path_buf();
    drop(inventario);
    let antes = bytes(&caminho);
    let inventario = Inventario::abrir(&caminho).unwrap();
    assert_eq!(inventario.estado(), EstadoDoBanco::Rust(2));
    assert!(!inventario.plano().unwrap().precisa_migrar());
    drop(inventario);
    assert_eq!(bytes(&caminho), antes);
}

#[test]
fn inv02_banco_do_bun_2_vira_user_version_2() {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    let c = inventario.conexao();
    assert_eq!(valor(c, "PRAGMA user_version"), "2");
    assert_eq!(inventario.estado(), EstadoDoBanco::Rust(2));
    // decidido_por veio de classificado_por / extraido_por.
    assert_eq!(
        valor(
            c,
            &format!(
                "SELECT decidido_por FROM achado WHERE content_hash = '{}'",
                hash(2)
            )
        ),
        "regras@2"
    );
    assert_eq!(
        valor(c, "SELECT decidido_por FROM registro WHERE id = 2"),
        "pluggy@1"
    );
    for indice in [
        "origem_gmail_message",
        "origem_local_filepath",
        "decisao_alvo",
    ] {
        assert_eq!(
            valor(
                c,
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = '{indice}'"
                )
            ),
            "1",
            "{indice}"
        );
    }
    // Os dados do Bun continuam lá.
    assert_eq!(valor(c, "SELECT COUNT(*) FROM origem"), "4");
}

#[test]
fn inv02_banco_vazio_e_banco_do_bun_terminam_com_o_mesmo_esquema() {
    let pasta = tempfile::tempdir().unwrap();
    let adotado = inventario_migrado(pasta.path());
    let vazio_caminho = pasta.path().join("novo.sqlite");
    let mut novo = Inventario::abrir(&vazio_caminho).unwrap();
    assert_eq!(novo.estado(), EstadoDoBanco::Vazio);
    let plano = novo.plano().unwrap();
    assert!(!plano.faz_copia);
    assert_eq!(plano.passos.len(), 2);
    let feita = novo.migrar(pasta.path().join("nao-usada.sqlite")).unwrap();
    assert_eq!(feita.copia, None);
    assert!(!pasta.path().join("nao-usada.sqlite").exists());
    assert_eq!(
        colunas_de_todas_as_tabelas(novo.conexao()),
        colunas_de_todas_as_tabelas(adotado.conexao())
    );
}

#[test]
fn inv02_rust_recusa_banco_mais_novo_sem_tocar_nele() {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    let caminho = inventario.caminho().to_path_buf();
    drop(inventario);
    Connection::open(&caminho)
        .unwrap()
        .execute_batch("PRAGMA user_version = 3")
        .unwrap();
    let antes = bytes(&caminho);
    match Inventario::abrir(&caminho) {
        Err(ErroDoBanco::MaisNovo {
            encontrada: 3,
            suportada: 2,
        }) => {}
        outro => panic!("esperava MaisNovo, veio {outro:?}"),
    }
    assert_eq!(bytes(&caminho), antes);
}

#[test]
fn inv02_banco_com_tabelas_que_nao_e_do_bun_e_recusado() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = pasta.path().join("outro.sqlite");
    Connection::open(&caminho)
        .unwrap()
        .execute_batch("CREATE TABLE qualquer (x); CREATE TABLE meta (chave TEXT, valor TEXT);")
        .unwrap();
    assert!(matches!(
        Inventario::abrir(&caminho),
        Err(ErroDoBanco::NaoEhDoBun { encontrado: None })
    ));
}

#[test]
fn inv02_banco_do_bun_sem_uma_coluna_da_base_e_recusado() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    Connection::open(&caminho)
        .unwrap()
        .execute_batch("ALTER TABLE achado DROP COLUMN notas")
        .unwrap();
    match Inventario::abrir(&caminho) {
        Err(ErroDoBanco::NaoBate { faltando }) => assert_eq!(faltando, vec!["achado.notas"]),
        outro => panic!("esperava NaoBate, veio {outro:?}"),
    }
}

#[test]
fn inv07_decisao_recusa_update_e_delete() {
    let pasta = tempfile::tempdir().unwrap();
    let mut inventario = inventario_migrado(pasta.path());
    let ids = inventario.executar(&marcar_documento(achado(1))).unwrap();
    assert_eq!(ids.len(), 1);
    let c = inventario.conexao();
    let update = c.execute("UPDATE decisao SET motivo = 'adulterado'", []);
    assert!(
        update
            .unwrap_err()
            .to_string()
            .contains("somente de acréscimo")
    );
    let delete = c.execute("DELETE FROM decisao", []);
    assert!(
        delete
            .unwrap_err()
            .to_string()
            .contains("somente de acréscimo")
    );
    let replace = c.execute(
        "INSERT OR REPLACE INTO decisao (id, alvo_tipo, alvo_id, campo, autor, em)
         VALUES (?1, 'achado', 'x', 'estado', 'usuario', 'agora')",
        [ids[0]],
    );
    assert!(replace.is_err(), "INSERT OR REPLACE furou o log");
    assert_eq!(valor(c, "SELECT COUNT(*) FROM decisao"), "1");
    assert_eq!(valor(c, "SELECT motivo FROM decisao"), "∅");
}

#[test]
fn leitura_separada_ve_a_decisao_e_nao_escreve() {
    let pasta = tempfile::tempdir().unwrap();
    let mut inventario = inventario_migrado(pasta.path());
    let leitura = sbf_inventario::banco::abrir_leitura(inventario.caminho()).unwrap();
    inventario.executar(&marcar_documento(achado(1))).unwrap();
    assert_eq!(
        valor(
            &leitura,
            &format!(
                "SELECT estado FROM achado WHERE content_hash = '{}'",
                hash(1)
            )
        ),
        "documento"
    );
    assert!(leitura.execute("DELETE FROM achado", []).is_err());
}

#[test]
fn migrar_com_andamento_avisa_cada_etapa_antes_de_comecar() {
    use sbf_inventario::banco::EtapaDaMigracao as E;
    use std::cell::RefCell;

    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_do_bun(pasta.path());
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    let etapas = RefCell::new(Vec::new());
    let copia = pasta.path().join("antes.sqlite");
    let feita = inventario
        .migrar_com_andamento(&copia, &|etapa| {
            // A cópia só existe depois da etapa de cópia.
            if etapa == E::Copia {
                assert!(!copia.exists());
            }
            etapas.borrow_mut().push(etapa);
        })
        .unwrap();
    assert!(feita.adotou_o_bun);
    assert_eq!(feita.versao_final, 2);
    let nomes: Vec<String> = etapas
        .borrow()
        .iter()
        .map(|e| match e {
            E::Migracao { versao, .. } => format!("Migracao{versao}"),
            outro => format!("{outro:?}"),
        })
        .collect();
    assert_eq!(
        nomes,
        [
            "Copia",
            "ConferenciaDaCopia",
            "Adocao",
            "Migracao2",
            "Conferencia"
        ]
    );

    // Banco que não existe: sem cópia e sem adoção; o arquivo nasce na migração.
    let novo = pasta.path().join("novo.sqlite");
    let mut inventario = Inventario::abrir(&novo).unwrap();
    let etapas = RefCell::new(Vec::new());
    inventario
        .migrar_com_andamento(pasta.path().join("x.sqlite"), &|e| {
            etapas.borrow_mut().push(e)
        })
        .unwrap();
    assert_eq!(etapas.borrow().len(), 3);
    assert!(matches!(etapas.borrow()[0], E::Migracao { versao: 1, .. }));
    assert_eq!(inventario.estado(), EstadoDoBanco::Rust(2));
    assert!(novo.exists());
    // A primeira decisão funciona no banco recém-criado.
    assert_eq!(
        valor(inventario.conexao(), "SELECT COUNT(*) FROM decisao"),
        "0"
    );
}

#[test]
fn inv01_reabrir_banco_migrado_e_decidir_abre_a_escrita_na_hora() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = inventario_migrado(pasta.path()).caminho().to_path_buf();
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    inventario.executar(&marcar_documento(achado(1))).unwrap();
    assert_eq!(
        valor(inventario.conexao(), "SELECT COUNT(*) FROM decisao"),
        "1"
    );
}
