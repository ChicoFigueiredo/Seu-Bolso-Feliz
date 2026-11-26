//! A coleta contra uma fonte falsa e um inventário temporário (nunca dados reais).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_telegram::coleta::{Opcoes, Reconhecer, coletar};
use sbf_telegram::config::Config;
use sbf_telegram::estado::Estado;
use sbf_telegram::modelo::{Avanco, Conversa, Midia, Pedido, Tipo};
use sbf_telegram::{Erro, FonteTelegram, Resultado};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

/// (id da mensagem, instante, nome, bytes)
type Entrada = (i64, u64, &'static str, &'static [u8]);

#[derive(Default)]
struct Falsa {
    conversas: Vec<Conversa>,
    midias: BTreeMap<String, Vec<Entrada>>,
    /// Ids de mensagem cujo download "falha".
    falham: Vec<i64>,
    /// Quantas vezes pediram cada conversa, e o `depois_do_id` do último pedido.
    pedidos: Vec<Pedido>,
    nao_autorizada: bool,
}

impl FonteTelegram for Falsa {
    fn listar_conversas(&mut self) -> Resultado<Vec<Conversa>> {
        if self.nao_autorizada {
            return Err(Erro::PrecisaEntrar);
        }
        Ok(self.conversas.clone())
    }

    fn baixar_midias(
        &mut self,
        pedido: &Pedido,
        pasta: &Path,
        ao_baixar: &mut dyn FnMut(Midia) -> Resultado<()>,
    ) -> Resultado<Avanco> {
        if self.nao_autorizada {
            return Err(Erro::PrecisaEntrar);
        }
        self.pedidos.push(pedido.clone());
        std::fs::create_dir_all(pasta)?;
        let mut ate = pedido.depois_do_id;
        let mut feitas = 0;
        for (id, quando, nome, bytes) in self.midias.get(&pedido.conversa).into_iter().flatten() {
            if *id <= pedido.depois_do_id || pedido.desde_data.is_some_and(|d| *quando < d) {
                ate = ate.max(*id);
                continue;
            }
            if feitas == pedido.limite {
                break;
            }
            if self.falham.contains(id) {
                return Err(Erro::Fonte("mídia expirada".into()));
            }
            let caminho = pasta.join(format!("{id}-{nome}"));
            std::fs::write(&caminho, bytes)?;
            ao_baixar(Midia {
                mensagem: *id,
                quando: *quando,
                nome: (*nome).to_owned(),
                caminho,
            })?;
            feitas += 1;
            ate = *id;
        }
        Ok(Avanco { ate_id: ate })
    }
}

#[derive(Default)]
struct Gravando {
    vistos: Vec<PathBuf>,
    falhar: bool,
}

impl Reconhecer for Gravando {
    fn reconhecer(&mut self, caminho: &Path) -> std::result::Result<(), String> {
        self.vistos.push(caminho.to_path_buf());
        if self.falhar {
            Err("falhou".into())
        } else {
            Ok(())
        }
    }
}

fn conversa(id: &str, nome: &str) -> Conversa {
    Conversa {
        id: id.into(),
        nome: nome.into(),
        tipo: Tipo::Grupo,
    }
}

struct Cenario {
    _d: tempfile::TempDir,
    cofre: PathBuf,
    conn: Connection,
    config: Config,
    estado: Estado,
}

fn cenario(seguindo: &[Conversa]) -> Cenario {
    let d = tempfile::tempdir().unwrap();
    let cofre = d.path().join("cofre");
    std::fs::create_dir_all(&cofre).unwrap();
    let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
    conn.execute_batch(SQL_BASE).unwrap();
    let mut config = Config::default();
    for c in seguindo {
        config.seguir(c);
    }
    Cenario {
        _d: d,
        cofre,
        conn,
        config,
        estado: Estado::default(),
    }
}

fn rodar(
    c: &mut Cenario,
    f: &mut Falsa,
    r: &mut Gravando,
    opcoes: &Opcoes,
) -> sbf_telegram::coleta::Relatorio {
    coletar(
        f,
        &c.conn,
        &c.cofre,
        &c.config,
        &mut c.estado,
        opcoes,
        r,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn contar(c: &Cenario, sql: &str) -> i64 {
    c.conn.query_row(sql, [], |l| l.get(0)).unwrap()
}

const PDF: &[u8] = b"%PDF-1.4 ficticio um";
const PDF2: &[u8] = b"%PDF-1.4 ficticio dois";

#[test]
fn baixa_para_a_entrada_registra_a_origem_e_reconhece_cada_arquivo_novo() {
    let casa = conversa("-1001", "Contas da Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias.insert(
        "-1001".into(),
        vec![
            (10, 1_700_000_000, "boleto.pdf", PDF),
            (11, 1_700_000_100, "foto.jpg", b"jpeg-ficticio"),
        ],
    );
    let mut r = Gravando::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());

    assert_eq!((rel.conversas, rel.novos, rel.repetidos), (1, 2, 0));
    assert_eq!(rel.reconhecidos, 2);
    let pasta = c.cofre.join("entrada/telegram/Contas_da_Casa-1001");
    assert!(pasta.join("10-boleto.pdf").is_file());
    assert!(pasta.join("11-foto.jpg").is_file());
    assert_eq!(r.vistos.len(), 2);
    assert!(r.vistos.iter().all(|p| p.starts_with(&pasta)));
    assert_eq!(contar(&c, "SELECT count(*) FROM achado"), 2);
    let chave: String = c
        .conn
        .query_row(
            "SELECT origin_key FROM origem WHERE filename = 'boleto.pdf'",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(chave, "telegram:-1001:10");
    assert_eq!(c.estado.ultimo_id("-1001"), 11);
    assert_eq!(c.estado.arquivos(), 2);
    assert!(c.estado.ultima_coleta.is_some());
}

#[test]
fn checkpoint_so_pede_o_que_e_novo_e_nao_repete() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias
        .insert("-1001".into(), vec![(10, 1_700_000_000, "a.pdf", PDF)]);
    let mut r = Gravando::default();
    rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    f.midias
        .get_mut("-1001")
        .unwrap()
        .push((12, 1_700_000_500, "b.pdf", PDF2));
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(f.pedidos.last().unwrap().depois_do_id, 10);
    assert_eq!(rel.novos, 1);
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 2);
    assert_eq!(c.estado.ultimo_id("-1001"), 12);
    // Sem novidade: nada muda.
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!((rel.novos, rel.repetidos), (0, 0));
}

#[test]
fn mesmo_arquivo_em_outra_mensagem_e_a_mesma_origem_a_mais_sem_novo_reconhecimento() {
    let a = conversa("-1", "A");
    let b = conversa("-2", "B");
    let mut c = cenario(&[a, b]);
    let mut f = Falsa::default();
    f.midias
        .insert("-1".into(), vec![(5, 1_700_000_000, "boleto.pdf", PDF)]);
    f.midias
        .insert("-2".into(), vec![(9, 1_700_000_900, "reenviado.pdf", PDF)]);
    let mut r = Gravando::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(contar(&c, "SELECT count(*) FROM achado"), 1);
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 2);
    assert_eq!(rel.reconhecidos, 1, "só o conteúdo novo é reconhecido");
    assert_eq!(rel.repetidos, 1);
    // A cópia repetida não fica na entrada.
    assert!(
        !c.cofre
            .join("entrada/telegram/B-2/9-reenviado.pdf")
            .exists()
    );
    assert_eq!(c.estado.ultimo_id("-2"), 9);
}

#[test]
fn estado_perdido_nao_duplica() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias
        .insert("-1001".into(), vec![(10, 1_700_000_000, "a.pdf", PDF)]);
    let mut r = Gravando::default();
    rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    c.estado = Estado::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(rel.novos, 0);
    assert_eq!(rel.repetidos, 1);
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 1);
    assert_eq!(r.vistos.len(), 1);
}

#[test]
fn desde_so_vale_na_primeira_coleta() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias.insert(
        "-1001".into(),
        vec![
            (1, 1_600_000_000, "velho.pdf", PDF),
            (2, 1_700_000_000, "novo.pdf", PDF2),
        ],
    );
    let mut r = Gravando::default();
    let o = Opcoes {
        desde_data: Some(1_650_000_000),
        ..Opcoes::default()
    };
    let rel = rodar(&mut c, &mut f, &mut r, &o);
    assert_eq!(rel.novos, 1);
    assert_eq!(f.pedidos[0].desde_data, Some(1_650_000_000));
    rodar(&mut c, &mut f, &mut r, &o);
    assert_eq!(
        f.pedidos[1].desde_data, None,
        "com checkpoint, quem manda é o id"
    );
}

#[test]
fn falha_numa_midia_para_so_aquela_conversa_e_o_checkpoint_fica_antes_dela() {
    let a = conversa("-1", "A");
    let b = conversa("-2", "B");
    let mut c = cenario(&[a, b]);
    let mut f = Falsa::default();
    f.midias.insert(
        "-1".into(),
        vec![
            (1, 1_700_000_000, "um.pdf", PDF),
            (2, 1_700_000_001, "dois.pdf", PDF2),
        ],
    );
    f.midias.insert(
        "-2".into(),
        vec![(7, 1_700_000_002, "tres.pdf", b"%PDF tres")],
    );
    f.falham.push(2);
    let mut r = Gravando::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(rel.erros, 1);
    assert_eq!(c.estado.ultimo_id("-1"), 1);
    assert_eq!(c.estado.ultimo_id("-2"), 7, "a outra conversa segue");
    assert!(c.estado.ultimo_erro.is_some());
    // Passou o problema: a próxima rodada pega a que faltou.
    f.falham.clear();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!((rel.novos, rel.erros), (1, 0));
    assert_eq!(c.estado.ultimo_id("-1"), 2);
    assert!(c.estado.ultimo_erro.is_none());
}

#[test]
fn falha_do_reconhecimento_nao_perde_o_achado() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias
        .insert("-1001".into(), vec![(10, 1_700_000_000, "a.pdf", PDF)]);
    let mut r = Gravando {
        falhar: true,
        ..Gravando::default()
    };
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(
        (rel.novos, rel.reconhecidos, rel.falhas_de_reconhecimento),
        (1, 0, 1)
    );
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 1);
    assert_eq!(c.estado.ultimo_id("-1001"), 10);
}

#[test]
fn limite_por_rodada_continua_na_proxima() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    c.config.lote = 1;
    let mut f = Falsa::default();
    f.midias.insert(
        "-1001".into(),
        vec![
            (1, 1_700_000_000, "a.pdf", PDF),
            (2, 1_700_000_001, "b.pdf", PDF2),
        ],
    );
    let mut r = Gravando::default();
    rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(c.estado.ultimo_id("-1001"), 1);
    rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(c.estado.ultimo_id("-1001"), 2);
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 2);
}

#[test]
fn nada_seguido_nem_pergunta_a_fonte() {
    let mut c = cenario(&[]);
    let mut f = Falsa {
        nao_autorizada: true,
        ..Falsa::default()
    };
    let mut r = Gravando::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert_eq!(rel.conversas, 0);
    assert!(f.pedidos.is_empty());
}

#[test]
fn sem_sessao_para_tudo_e_avisa() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa {
        nao_autorizada: true,
        ..Falsa::default()
    };
    let mut r = Gravando::default();
    let rel = rodar(&mut c, &mut f, &mut r, &Opcoes::default());
    assert!(rel.precisa_entrar);
    assert!(c.estado.ultimo_erro.is_some());
}

#[test]
fn so_uma_conversa_quando_pedido() {
    let a = conversa("-1", "A");
    let b = conversa("-2", "B");
    let mut c = cenario(&[a, b]);
    let mut f = Falsa::default();
    f.midias
        .insert("-1".into(), vec![(1, 1_700_000_000, "a.pdf", PDF)]);
    f.midias
        .insert("-2".into(), vec![(1, 1_700_000_000, "b.pdf", PDF2)]);
    let mut r = Gravando::default();
    let o = Opcoes {
        so_conversa: Some("-2".into()),
        ..Opcoes::default()
    };
    let rel = rodar(&mut c, &mut f, &mut r, &o);
    assert_eq!(rel.novos, 1);
    assert_eq!(f.pedidos.len(), 1);
    assert_eq!(f.pedidos[0].conversa, "-2");
}

#[test]
fn cancelar_para_e_marca_interrompido() {
    let casa = conversa("-1001", "Casa");
    let mut c = cenario(std::slice::from_ref(&casa));
    let mut f = Falsa::default();
    f.midias
        .insert("-1001".into(), vec![(10, 1_700_000_000, "a.pdf", PDF)]);
    let mut r = Gravando::default();
    let rel = coletar(
        &mut f,
        &c.conn,
        &c.cofre,
        &c.config,
        &mut c.estado,
        &Opcoes::default(),
        &mut r,
        &AtomicBool::new(true),
    )
    .unwrap();
    assert!(rel.interrompido);
    assert_eq!(contar(&c, "SELECT count(*) FROM origem"), 0);
}
