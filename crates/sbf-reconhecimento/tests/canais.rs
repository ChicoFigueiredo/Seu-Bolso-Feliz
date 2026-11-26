//! O reconhecimento por canal (Gmail): o arquivo e a mensagem chegam dizendo de onde vieram e
//! as regras veem `Entrada.origem`. Cofre temporário, dados fictícios, nenhuma IA.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use sbf_reconhecimento::regras::MensagemRecebida;
use sbf_reconhecimento::{ReconhecedorDoCofre, Reconhecido};
use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::{Condicao, Definicao};
use sbf_regras::{Autor, Canal, Quem, TipoDeEntrada};
use zeroize::Zeroizing;

const NFE: &str = "<nfeProc><NFe><infNFe><emit><xNome>Loja Exemplo Ltda</xNome></emit>\
<total><vNF>187.50</vNF></total></infNFe></NFe></nfeProc>";
const AVISO: &str = "Aviso de boleto\nBanco Exemplo informa: seu boleto vence em 24/11/2026.";

struct SemSegredos;

impl FonteDeSegredos for SemSegredos {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
    fn senha_propria(&self, _: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::Indisponivel)
    }
}

struct Cenario {
    _raiz: tempfile::TempDir,
    cofre: PathBuf,
}

fn cenario() -> Cenario {
    let raiz = tempfile::tempdir().expect("raiz");
    let cofre = raiz.path().join("cofre");
    std::fs::create_dir_all(cofre.join("entrada/gmail")).expect("cofre");
    let mut inventario = Inventario::abrir(cofre.join("inventario.sqlite")).expect("abrir");
    inventario
        .migrar(raiz.path().join("copia.sqlite"))
        .expect("migrar");
    Cenario { _raiz: raiz, cofre }
}

impl Cenario {
    fn reconhecedor(&self) -> ReconhecedorDoCofre {
        ReconhecedorDoCofre::novo(&self.cofre, Arc::new(SemSegredos), None).com_regras(None)
    }

    fn instalar(&self, regra: &str, tipos: Vec<TipoDeEntrada>, contem: &str) {
        let mut d = Definicao::simples("nota-fiscal", contem);
        d.aplica_a = tipos;
        d.detectar.todos.push(Condicao::Canal(vec![Canal::Gmail]));
        let mut c = Catalogo::abrir(&self.cofre.join("regras.sqlite")).expect("catalogo");
        c.semear_fabrica().expect("fabrica");
        let v = c
            .salvar_rascunho(NovaVersao {
                regra: regra.to_owned(),
                nome: regra.to_owned(),
                definicao: d,
                script: None,
                autor: Autor::Usuario,
                motivo: "teste".to_owned(),
            })
            .expect("rascunho");
        c.ativar(&v.regra, v.versao, &Quem::Usuario, None)
            .expect("ativar");
    }

    /// Registra o achado como o coletor do Gmail faz e devolve o caminho dentro do cofre.
    fn guardar(&self, nome: &str, conteudo: &str) -> PathBuf {
        let caminho = self.cofre.join("entrada/gmail").join(nome);
        std::fs::write(&caminho, conteudo).expect("escrever");
        let hash = sbf_coleta::pastas::sha256_do_arquivo(&caminho).expect("sha");
        let conn = Connection::open(self.cofre.join("inventario.sqlite")).expect("abrir");
        conn.execute(
            "INSERT OR IGNORE INTO achado (content_hash, file_size_bytes, camada, caminho_acervo)
             VALUES (?1, ?2, 'A', ?3)",
            rusqlite::params![
                hash,
                i64::try_from(conteudo.len()).unwrap(),
                format!("entrada/gmail/{nome}")
            ],
        )
        .expect("achado");
        caminho
    }
}

fn reconhecido_por(regra: &str, r: &Reconhecido) -> bool {
    matches!(r, Reconhecido::PelaRegra { reconhecimento } if reconhecimento.regra == regra)
}

#[test]
fn anexo_xml_do_gmail_passa_pelas_regras_com_o_canal_gmail() {
    let c = cenario();
    c.instalar("nfe-exemplo", vec![TipoDeEntrada::Documento], "nfeproc");
    let caminho = c.guardar("nfe.xml", NFE);

    let r = c
        .reconhecedor()
        .reconhecer_de(
            Some((Canal::Gmail, "msg1".to_owned())),
            &caminho,
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .expect("reconhecer");

    assert!(
        reconhecido_por("nfe-exemplo", &r.reconhecido),
        "{:?}",
        r.reconhecido
    );
    assert!(r.ja_existia, "o coletor registrou o achado antes");
    let entrada = r.entrada.expect("entrada");
    assert_eq!(entrada.origem.canal, Canal::Gmail);
    assert_eq!(entrada.origem.id_externo.as_deref(), Some("msg1"));
}

#[test]
fn a_regra_so_do_gmail_nao_pega_o_mesmo_arquivo_solto() {
    let c = cenario();
    c.instalar("nfe-exemplo", vec![TipoDeEntrada::Documento], "nfeproc");
    let caminho = c.guardar("nfe.xml", NFE);

    let r = c
        .reconhecedor()
        .reconhecer_de(None, &caminho, &mut |_| {}, &AtomicBool::new(false))
        .expect("reconhecer");

    assert!(!reconhecido_por("nfe-exemplo", &r.reconhecido));
}

#[test]
fn corpo_de_email_sem_anexo_vira_entrada_mensagem_e_passa_pelas_regras() {
    let c = cenario();
    c.instalar(
        "aviso-de-boleto",
        vec![TipoDeEntrada::Mensagem],
        "boleto vence",
    );

    let r = c
        .reconhecedor()
        .reconhecer_mensagem(&MensagemRecebida {
            canal: Canal::Gmail,
            id: "gmail:msg2",
            id_externo: "msg2",
            texto: AVISO,
            momento: Some("2026-11-17T12:00:00Z"),
            data: Some("2026-11-17"),
        })
        .expect("mensagem");

    assert!(reconhecido_por("aviso-de-boleto", &r), "{r:?}");
    // O rastro da execução ficou no catálogo.
    let conn = Connection::open(c.cofre.join("regras.sqlite")).expect("abrir");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM execucao", [], |l| l.get(0))
        .expect("execucoes");
    assert!(n > 0);
}

#[test]
fn mensagem_sem_regras_ligadas_ou_sem_regra_que_sirva_e_nada() {
    let c = cenario();
    let sem = ReconhecedorDoCofre::novo(&c.cofre, Arc::new(SemSegredos), None);
    let m = MensagemRecebida {
        canal: Canal::Gmail,
        id: "gmail:m3",
        id_externo: "m3",
        texto: "bom dia",
        momento: None,
        data: None,
    };
    assert!(matches!(
        sem.reconhecer_mensagem(&m).expect("ok"),
        Reconhecido::Nada { .. }
    ));
    assert!(matches!(
        c.reconhecedor().reconhecer_mensagem(&m).expect("ok"),
        Reconhecido::Nada { .. }
    ));
}

#[allow(dead_code)]
fn _usa(_: &Path) {}
