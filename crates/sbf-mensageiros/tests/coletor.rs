//! O ciclo de coleta contra uma fonte falsa e um inventário temporário (nunca dados reais).

#![allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_mensageiros::coletor::{Pastas, coletar};
use sbf_mensageiros::config::{Checkpoints, Config};
use sbf_mensageiros::modelo::{Anexo, Checkpoint, Conversa, Mensagem, Plataforma};
use sbf_mensageiros::{Erro, FonteDeMensagens, Resultado};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

#[derive(Default)]
struct Falsa {
    conversas: Vec<Conversa>,
    mensagens: BTreeMap<String, Vec<Mensagem>>,
    midias: BTreeMap<String, Vec<u8>>,
    /// Ids de mensagem cuja mídia "expirou".
    expiradas: Vec<String>,
    pediu_mensagens: Vec<String>,
    baixou: usize,
}

impl FonteDeMensagens for Falsa {
    fn plataforma(&self) -> Plataforma {
        Plataforma::Whatsapp
    }
    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>> {
        Ok(self.conversas.clone())
    }
    fn mensagens_novas(
        &mut self,
        conversa: &str,
        desde: &Checkpoint,
        limite: usize,
    ) -> Resultado<Vec<Mensagem>> {
        self.pediu_mensagens.push(conversa.to_owned());
        Ok(self
            .mensagens
            .get(conversa)
            .into_iter()
            .flatten()
            .filter(|m| desde.antes_de(m))
            .take(limite)
            .cloned()
            .collect())
    }
    fn baixar_anexo(&mut self, m: &Mensagem, destino: &Path) -> Resultado<()> {
        if self.expiradas.contains(&m.id) {
            return Err(Erro::Fonte("mídia expirada".into()));
        }
        self.baixou += 1;
        std::fs::write(destino, &self.midias[&m.id])?;
        Ok(())
    }
}

fn conversa(id: &str) -> Conversa {
    Conversa {
        id: id.into(),
        nome: format!("Conversa {id}"),
        grupo: false,
    }
}

fn msg(id: &str, chat: &str, quando: u64, texto: Option<&str>, anexo: Option<&str>) -> Mensagem {
    Mensagem {
        id: id.into(),
        conversa: chat.into(),
        quando,
        texto: texto.map(str::to_owned),
        anexo: anexo.map(|n| Anexo {
            nome: n.into(),
            mime: None,
            tamanho: None,
        }),
    }
}

struct Cenario {
    _d: tempfile::TempDir,
    cofre: std::path::PathBuf,
    conn: Connection,
    config: Config,
}

fn cenario() -> Cenario {
    let d = tempfile::tempdir().unwrap();
    let cofre = d.path().join("cofre");
    std::fs::create_dir_all(&cofre).unwrap();
    let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
    conn.execute_batch(SQL_BASE).unwrap();
    let mut config = Config::default();
    config.ligado = true;
    config.whatsapp.ligada = true;
    Cenario {
        _d: d,
        cofre,
        conn,
        config,
    }
}

fn rodar(
    c: &Cenario,
    f: &mut Falsa,
    pontos: &mut Checkpoints,
) -> sbf_mensageiros::coletor::Relatorio {
    coletar(
        f,
        &c.conn,
        &Pastas {
            cofre: c.cofre.clone(),
        },
        &c.config,
        pontos,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn contar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |l| l.get(0)).unwrap()
}

fn fonte_com_duas_conversas() -> Falsa {
    let mut f = Falsa::default();
    f.conversas = vec![conversa("familia"), conversa("trabalho")];
    f.mensagens.insert(
        "familia".into(),
        vec![
            msg("1", "familia", 100, Some("bom dia"), None),
            msg("2", "familia", 200, None, Some("boleto-luz.pdf")),
            msg(
                "3",
                "familia",
                300,
                Some("segue o comprovante do pix"),
                None,
            ),
        ],
    );
    f.mensagens.insert(
        "trabalho".into(),
        vec![msg("9", "trabalho", 150, None, Some("fatura.pdf"))],
    );
    f.midias
        .insert("2".into(), b"%PDF-1.4 boleto ficticio".to_vec());
    f.midias
        .insert("9".into(), b"%PDF-1.4 fatura ficticia".to_vec());
    f
}

#[test]
fn padrao_nao_le_nenhuma_conversa_nem_pergunta_a_fonte() {
    let c = cenario();
    let mut f = fonte_com_duas_conversas();
    let mut pontos = Checkpoints::default();
    let r = rodar(&c, &mut f, &mut pontos);
    assert_eq!(r.mensagens, 0);
    assert!(f.pediu_mensagens.is_empty());
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 0);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM varredura"), 0);
}

#[test]
fn so_a_conversa_incluida_vira_achado_com_origem_e_acervo() {
    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    let mut f = fonte_com_duas_conversas();
    let mut pontos = Checkpoints::default();
    let r = rodar(&c, &mut f, &mut pontos);
    assert_eq!(r.conversas, 1);
    assert_eq!(f.pediu_mensagens, ["familia"]);
    // anexo pdf + texto financeiro; "bom dia" fica de fora
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 2);
    let chaves: Vec<String> = c
        .conn
        .prepare("SELECT origin_key FROM origem ORDER BY origin_key")
        .unwrap()
        .query_map([], |l| l.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(chaves, ["whatsapp:familia/2", "whatsapp:familia/3"]);
    let (tipo, ref_): (String, String) = c
        .conn
        .query_row(
            "SELECT origin_type, mensagem_ref FROM origem WHERE origin_key = 'whatsapp:familia/2'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .unwrap();
    assert_eq!((tipo.as_str(), ref_.as_str()), ("whatsapp", "familia/2"));
    assert_eq!(
        contar(
            &c.conn,
            "SELECT count(*) FROM achado WHERE caminho_acervo IS NOT NULL AND data_chave_tipo = 'mensagem'"
        ),
        2
    );
    let caminho: String = c
        .conn
        .query_row(
            "SELECT caminho_acervo FROM achado WHERE extensao = 'pdf'",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert!(c.cofre.join(caminho).is_file());
    assert_eq!(
        contar(
            &c.conn,
            "SELECT count(*) FROM fonte WHERE id = 'mensageiro:whatsapp' AND tipo = 'mensageiro'"
        ),
        1
    );
    // o temporário não fica
    assert_eq!(
        std::fs::read_dir(c.cofre.join("tmp/mensageiros"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn segunda_rodada_usa_o_checkpoint_e_nao_duplica() {
    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    let mut f = fonte_com_duas_conversas();
    let mut pontos = Checkpoints::default();
    rodar(&c, &mut f, &mut pontos);
    assert_eq!(f.baixou, 1);
    assert_eq!(pontos.de(Plataforma::Whatsapp, "familia").quando, 300);
    let r = rodar(&c, &mut f, &mut pontos);
    assert_eq!(r.mensagens, 0);
    assert_eq!(f.baixou, 1);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 2);
    // perdeu o checkpoint: relê, mas a origem já existe e a mídia nem é baixada de novo
    let mut vazio = Checkpoints::default();
    let r = rodar(&c, &mut f, &mut vazio);
    assert_eq!(r.mensagens, 3);
    assert_eq!(f.baixou, 1);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 2);
}

#[test]
fn mesmo_conteudo_em_duas_conversas_e_um_achado_com_duas_origens() {
    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    c.config.whatsapp.incluir("trabalho", true);
    let mut f = fonte_com_duas_conversas();
    f.midias.insert("9".into(), f.midias["2"].clone());
    f.mensagens.get_mut("trabalho").unwrap()[0]
        .anexo
        .as_mut()
        .unwrap()
        .nome = "boleto-luz.pdf".into();
    let mut pontos = Checkpoints::default();
    rodar(&c, &mut f, &mut pontos);
    assert_eq!(
        contar(
            &c.conn,
            "SELECT count(*) FROM achado WHERE extensao = 'pdf'"
        ),
        1
    );
    assert_eq!(
        contar(
            &c.conn,
            "SELECT count(*) FROM origem WHERE content_hash IN (SELECT content_hash FROM achado WHERE extensao='pdf')"
        ),
        2
    );
}

#[test]
fn exclusao_ganha_e_midia_expirada_para_antes_dela_sem_perder_o_resto() {
    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    c.config.whatsapp.excluidas.push("familia".into());
    let mut f = fonte_com_duas_conversas();
    let mut pontos = Checkpoints::default();
    assert_eq!(rodar(&c, &mut f, &mut pontos).mensagens, 0);

    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    let mut f = fonte_com_duas_conversas();
    f.expiradas.push("2".into());
    let mut pontos = Checkpoints::default();
    let r = rodar(&c, &mut f, &mut pontos);
    assert_eq!(r.contadores.erros, 1);
    // parou na mensagem 2: o checkpoint é o da mensagem 1 e a 3 espera
    assert_eq!(pontos.de(Plataforma::Whatsapp, "familia").quando, 100);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 0);
    f.expiradas.clear();
    rodar(&c, &mut f, &mut pontos);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 2);
}

#[test]
fn cancelar_antes_de_comecar_nao_le_nada() {
    let mut c = cenario();
    c.config.whatsapp.incluir("familia", true);
    let mut f = fonte_com_duas_conversas();
    let mut pontos = Checkpoints::default();
    let cancelar = AtomicBool::new(true);
    let r = coletar(
        &mut f,
        &c.conn,
        &Pastas {
            cofre: c.cofre.clone(),
        },
        &c.config,
        &mut pontos,
        &cancelar,
    )
    .unwrap();
    assert!(r.interrompido);
    assert_eq!(contar(&c.conn, "SELECT count(*) FROM origem"), 0);
}
