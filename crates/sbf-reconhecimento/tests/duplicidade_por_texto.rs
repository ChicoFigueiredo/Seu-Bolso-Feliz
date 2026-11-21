//! O mesmo documento que chega com bytes diferentes e texto idêntico (a Stripe gera o PDF de
//! novo a cada envio; o mesmo recibo chega por e-mail e pelo Telegram): o mais antigo segue
//! como está, o mais novo vira "Possível duplicidade" e diz de quem. Cofre temporário, dados
//! fictícios.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_inventario::caixa_de_entrada::{self, Canal, Estado, Filtro};
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_reconhecimento::{Reconhecedor, ReconhecedorDoCofre};
use zeroize::Zeroizing;

const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");

struct Segredos;

impl FonteDeSegredos for Segredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new("52998224725".to_owned()))
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::SemSenhaPropria)
    }
}

struct Cenario {
    _raiz: tempfile::TempDir,
    cofre: PathBuf,
    soltos: PathBuf,
}

fn cenario() -> Cenario {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    let soltos = raiz.path().join("soltos");
    std::fs::create_dir_all(&cofre).expect("cofre");
    std::fs::create_dir_all(&soltos).expect("soltos");
    let mut inventario = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inventario
        .migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    Cenario {
        _raiz: raiz,
        cofre,
        soltos,
    }
}

impl Cenario {
    fn conexao(&self) -> Connection {
        Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir")
    }

    fn solto(&self, nome: &str, conteudo: &str) -> PathBuf {
        let c = self.soltos.join(nome);
        std::fs::write(&c, conteudo).expect("escrever");
        c
    }

    fn reconhecedor(&self, canal: Canal) -> ReconhecedorDoCofre {
        ReconhecedorDoCofre::novo(&self.cofre, Arc::new(Segredos), None)
            .com_caixa_de_entrada(canal, false)
    }

    fn rodar(&self, r: &ReconhecedorDoCofre, caminho: &Path) -> sbf_reconhecimento::Resultado {
        r.reconhecer(caminho, &mut |_| {}, &AtomicBool::new(false))
            .expect("reconhecer")
    }

    fn entrada(&self, sha: &str) -> caixa_de_entrada::Entrada {
        caixa_de_entrada::obter(&self.conexao(), sha)
            .expect("obter")
            .expect("na caixa")
    }
}

/// O mesmo texto com bytes diferentes (o PDF regerado a cada envio).
fn regerado() -> String {
    format!("{CONTRACHEQUE}\r\n\r\n   \n")
}

#[test]
fn o_segundo_com_o_mesmo_texto_vira_possivel_duplicidade_e_diz_de_quem() {
    let c = cenario();
    let a = c.solto("recibo-do-email.txt", CONTRACHEQUE);
    let b = c.solto("recibo-do-telegram.txt", &regerado());
    let primeiro = c.rodar(&c.reconhecedor(Canal::Gmail), &a);
    let segundo = c.rodar(&c.reconhecedor(Canal::Telegram), &b);
    assert_ne!(primeiro.sha256, segundo.sha256);

    let velho = c.entrada(&primeiro.sha256);
    assert_ne!(velho.estado, Estado::PossivelDuplicidade);
    assert!(
        !velho
            .resumo
            .as_deref()
            .unwrap_or_default()
            .contains("mesmo texto"),
        "{:?}",
        velho.resumo
    );

    let novo = c.entrada(&segundo.sha256);
    assert_eq!(novo.estado, Estado::PossivelDuplicidade);
    let resumo = novo.resumo.expect("resumo");
    assert!(
        resumo.contains("mesmo texto de recibo-do-email.txt"),
        "{resumo}"
    );
    assert!(resumo.starts_with("Contracheque"), "{resumo}");
}

#[test]
fn reconhecer_de_novo_o_mais_antigo_nao_o_marca_duplicado() {
    let c = cenario();
    let a = c.solto("a.txt", CONTRACHEQUE);
    let b = c.solto("b.txt", &regerado());
    let primeiro = c.rodar(&c.reconhecedor(Canal::Gmail), &a);
    c.rodar(&c.reconhecedor(Canal::Telegram), &b);
    // O usuário aperta "Reconhecer de novo" no mais antigo.
    c.rodar(
        &c.reconhecedor(Canal::Gmail)
            .atualizando_a_caixa_de_entrada(),
        &a,
    );
    assert_ne!(
        c.entrada(&primeiro.sha256).estado,
        Estado::PossivelDuplicidade
    );
    let reconhecidos = c.fila(&Filtro::caixa());
    assert_eq!(
        reconhecidos
            .iter()
            .filter(|e| e.estado == Estado::PossivelDuplicidade)
            .count(),
        1
    );
}

#[test]
fn reconhecer_de_novo_um_repetido_antigo_ganha_o_estado_novo() {
    let c = cenario();
    let a = c.solto("a.txt", CONTRACHEQUE);
    let b = c.solto("b.txt", &regerado());
    // Antes desta função existir, os dois ficaram como itens independentes: simula tirando
    // o estado de duplicidade do mais novo.
    c.rodar(&c.reconhecedor(Canal::Gmail), &a);
    let segundo = c.rodar(&c.reconhecedor(Canal::Telegram), &b);
    c.conexao()
        .execute(
            "UPDATE caixa_entrada SET estado = 'pronto', resumo = 'Contracheque' WHERE content_hash = ?1",
            [&segundo.sha256],
        )
        .expect("voltar ao estado antigo");
    c.rodar(
        &c.reconhecedor(Canal::Telegram)
            .atualizando_a_caixa_de_entrada(),
        &b,
    );
    assert_eq!(
        c.entrada(&segundo.sha256).estado,
        Estado::PossivelDuplicidade
    );
}

#[test]
fn texto_gravado_pelo_bun_sem_impressao_tambem_conta() {
    let c = cenario();
    // O achado antigo veio dos scripts Bun: texto direto no banco, impressão pendente.
    let con = c.conexao();
    con.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, primeiro_visto_em)
         VALUES (?1, 10, 'A', '2026-01-01T10:00:00.000Z')",
        [&"ab".repeat(32)],
    )
    .expect("achado");
    con.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
        rusqlite::params!["ab".repeat(32), CONTRACHEQUE],
    )
    .expect("texto");
    let novo = c.solto("novo.txt", &regerado());
    let r = c.rodar(&c.reconhecedor(Canal::Telegram), &novo);
    let e = c.entrada(&r.sha256);
    assert_eq!(e.estado, Estado::PossivelDuplicidade);
    // Sem Caixa nem origem com nome, aponta pelo começo do hash.
    assert!(
        e.resumo
            .as_deref()
            .unwrap_or_default()
            .contains("mesmo texto de abababababab"),
        "{:?}",
        e.resumo
    );
}

impl Cenario {
    fn fila(&self, filtro: &Filtro) -> Vec<caixa_de_entrada::Entrada> {
        caixa_de_entrada::listar(&self.conexao(), filtro).expect("listar")
    }
}
